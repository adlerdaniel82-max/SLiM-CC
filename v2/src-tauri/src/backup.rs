use crate::error::{SlimError, SlimResult};
use crate::models::BackupSnapshot;
use chrono::Utc;
use serde_json;
use std::fs;
use std::path::Path;
use uuid::Uuid;
use walkdir::WalkDir;

pub fn create_backup_snapshot(
    backups_root: &Path,
    instance_id: &str,
    profile_id: Option<&str>,
    source_root: &Path,
    label: &str,
) -> SlimResult<BackupSnapshot> {
    let snapshot_id = Uuid::new_v4().to_string();
    let snapshot_root = backups_root.join(instance_id).join(&snapshot_id);
    let contents_root = snapshot_root.join("contents");
    fs::create_dir_all(&contents_root)?;

    let mut item_count = 0usize;
    if source_root.exists() {
        item_count = copy_tree(source_root, &contents_root)?;
    }

    let snapshot = BackupSnapshot {
        id: snapshot_id,
        instance_id: instance_id.to_string(),
        profile_id: profile_id.map(str::to_string),
        label: label.trim().to_string(),
        source_root: source_root.to_path_buf(),
        snapshot_root: snapshot_root.clone(),
        created_at: Utc::now().to_rfc3339(),
        item_count,
    };

    fs::create_dir_all(&snapshot_root)?;
    fs::write(
        snapshot_root.join("manifest.json"),
        serde_json::to_string_pretty(&snapshot)?,
    )?;

    Ok(snapshot)
}

pub fn restore_backup_snapshot(snapshot: &BackupSnapshot, target_root: &Path) -> SlimResult<()> {
    let contents_root = snapshot.snapshot_root.join("contents");
    if !contents_root.exists() {
        return Err(SlimError::NotFound(format!(
            "snapshot contents missing: {}",
            contents_root.display()
        )));
    }

    if target_root.exists() {
        fs::remove_dir_all(target_root)?;
    }
    fs::create_dir_all(target_root)?;
    copy_tree(&contents_root, target_root)?;
    Ok(())
}

pub fn read_backup_snapshot(snapshot_root: &Path) -> SlimResult<BackupSnapshot> {
    let manifest_path = snapshot_root.join("manifest.json");
    let content = fs::read_to_string(&manifest_path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn list_backup_snapshots(
    backups_root: &Path,
    instance_id: &str,
) -> SlimResult<Vec<BackupSnapshot>> {
    let instance_root = backups_root.join(instance_id);
    if !instance_root.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();
    for entry in fs::read_dir(instance_root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join("manifest.json");
        if manifest.is_file() {
            snapshots.push(read_backup_snapshot(&path)?);
        }
    }

    snapshots.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(snapshots)
}

fn copy_tree(source_root: &Path, target_root: &Path) -> SlimResult<usize> {
    if !source_root.exists() {
        return Ok(0);
    }

    let mut count = 0usize;
    for entry in WalkDir::new(source_root).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source_root)
            .map_err(|_| SlimError::Safety("failed to compute snapshot relative path".into()))?;
        let target = target_root.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &target)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("slimcc-backup-{label}-{stamp}"))
    }

    #[test]
    fn create_and_restore_backup_snapshot_round_trips_contents() {
        let workspace_root = temp_dir("workspace");
        let backups_root = workspace_root
            .join("instances")
            .join("instance-a")
            .join("backups");
        let source_root = workspace_root.join("source");
        let target_root = workspace_root.join("target");
        fs::create_dir_all(source_root.join("textures")).expect("create source");
        fs::write(source_root.join("textures").join("foo.dds"), b"abc").expect("write file");

        let snapshot = create_backup_snapshot(
            &backups_root,
            "instance-a",
            Some("profile-a"),
            &source_root,
            "pre-deploy",
        )
        .expect("create snapshot");

        fs::create_dir_all(&target_root).expect("create target");
        fs::write(target_root.join("stale.txt"), b"stale").expect("write stale");
        restore_backup_snapshot(&snapshot, &target_root).expect("restore snapshot");

        assert!(target_root.join("textures").join("foo.dds").is_file());
        assert!(!target_root.join("stale.txt").exists());
        assert!(snapshot.snapshot_root.join("manifest.json").is_file());
    }
}
