use crate::error::{SlimError, SlimResult};
use std::path::{Path, PathBuf};

pub fn workspace_root() -> SlimResult<PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| SlimError::InvalidPath("could not resolve local data directory".into()))?;
    Ok(base.join("slim-cc"))
}

pub fn database_path(root: &Path) -> PathBuf {
    root.join("slimcc.sqlite")
}

pub fn instance_root(root: &Path, instance_id: &str) -> PathBuf {
    root.join("instances").join(instance_id)
}

pub fn mods_root(root: &Path, instance_id: &str) -> PathBuf {
    instance_root(root, instance_id).join("mods")
}

pub fn profiles_root(root: &Path, instance_id: &str) -> PathBuf {
    instance_root(root, instance_id).join("profiles")
}

pub fn logs_root(root: &Path, instance_id: &str) -> PathBuf {
    instance_root(root, instance_id).join("logs")
}

pub fn backups_root(root: &Path, instance_id: &str) -> PathBuf {
    instance_root(root, instance_id).join("backups")
}

pub fn staging_data_path(root: &Path, instance_id: &str) -> PathBuf {
    instance_root(root, instance_id)
        .join("staging")
        .join("Data")
}

pub fn vfs_root(root: &Path, instance_id: &str, profile_id: &str) -> PathBuf {
    profile_path(root, instance_id, profile_id).join("vfs")
}

pub fn vfs_layer_data_path(root: &Path, instance_id: &str, profile_id: &str) -> PathBuf {
    vfs_layer_path(root, instance_id, profile_id).join("Data")
}

pub fn vfs_layer_path(root: &Path, instance_id: &str, profile_id: &str) -> PathBuf {
    vfs_root(root, instance_id, profile_id).join("layer")
}

pub fn vfs_mount_path(root: &Path, instance_id: &str, profile_id: &str) -> PathBuf {
    vfs_root(root, instance_id, profile_id).join("game")
}

pub fn loot_data_path(root: &Path, instance_id: &str) -> PathBuf {
    instance_root(root, instance_id).join("loot-data")
}

pub fn profile_path(root: &Path, instance_id: &str, profile_id: &str) -> PathBuf {
    instance_root(root, instance_id)
        .join("profiles")
        .join(profile_id)
}

pub fn ensure_workspace(root: &Path) -> SlimResult<()> {
    std::fs::create_dir_all(root.join("instances"))?;
    Ok(())
}

pub fn guard_inside(base: &Path, candidate: &Path) -> SlimResult<()> {
    let base_canon = std::fs::canonicalize(base)?;
    let candidate_parent = candidate.parent().ok_or_else(|| {
        SlimError::InvalidPath(format!("path has no parent: {}", candidate.display()))
    })?;
    std::fs::create_dir_all(candidate_parent)?;
    let parent_canon = std::fs::canonicalize(candidate_parent)?;

    if !parent_canon.starts_with(&base_canon) {
        return Err(SlimError::Safety(format!(
            "refusing write outside guarded path: base={}, candidate={}",
            base_canon.display(),
            candidate.display()
        )));
    }

    Ok(())
}

pub fn guard_descendant(base: &Path, candidate: &Path) -> SlimResult<()> {
    let base_canon = std::fs::canonicalize(base)?;
    let mut probe = if candidate.exists() {
        candidate
    } else {
        candidate.parent().ok_or_else(|| {
            SlimError::InvalidPath(format!("path has no parent: {}", candidate.display()))
        })?
    };
    while !probe.exists() {
        probe = probe.parent().ok_or_else(|| {
            SlimError::InvalidPath(format!(
                "path has no existing ancestor: {}",
                candidate.display()
            ))
        })?;
    }
    let probe_canon = std::fs::canonicalize(probe)?;

    if !probe_canon.starts_with(&base_canon) {
        return Err(SlimError::Safety(format!(
            "refusing write outside guarded path: base={}, candidate={}",
            base_canon.display(),
            candidate.display()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("slim-cc-{label}-{stamp}"))
    }

    #[test]
    fn guard_descendant_allows_descendants() {
        let base = temp_dir("inside");
        let child = base.join("nested").join("file.txt");
        fs::create_dir_all(child.parent().expect("child parent")).expect("create child parent");
        fs::create_dir_all(&base).expect("create base");
        fs::write(child.parent().expect("child parent").join(".probe"), b"x").expect("probe write");

        assert!(guard_descendant(&base, &child).is_ok());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn guard_descendant_allows_missing_nested_descendants() {
        let base = temp_dir("missing-inside");
        let child = base.join("nested").join("deeper").join("file.txt");
        fs::create_dir_all(&base).expect("create base");

        assert!(guard_descendant(&base, &child).is_ok());

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn guard_descendant_rejects_outside_paths() {
        let base = temp_dir("outside-base");
        let outside = temp_dir("outside-child").join("file.txt");
        fs::create_dir_all(&base).expect("create base");
        fs::create_dir_all(outside.parent().expect("outside parent"))
            .expect("create outside parent");
        fs::write(
            outside.parent().expect("outside parent").join(".probe"),
            b"x",
        )
        .expect("probe write");

        assert!(guard_descendant(&base, &outside).is_err());

        fs::remove_dir_all(&base).ok();
        fs::remove_dir_all(outside.parent().expect("outside parent")).ok();
    }
}
