use crate::error::{SlimError, SlimResult};
use crate::models::VfsStatus;
use crate::paths;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

pub struct VfsManager {
    mounts: BTreeMap<String, Child>,
}

impl VfsManager {
    pub fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
        }
    }

    pub fn mount(
        &mut self,
        workspace_root: &Path,
        instance_id: &str,
        profile_id: &str,
        game_root: &Path,
    ) -> SlimResult<VfsStatus> {
        ensure_linux_vfs_available()?;
        if !game_root.is_dir() {
            return Err(SlimError::InvalidPath(format!(
                "game root does not exist: {}",
                game_root.display()
            )));
        }
        let key = mount_key(instance_id, profile_id);
        let root = paths::vfs_root(workspace_root, instance_id, profile_id);
        let layer = root.join("layer");
        let upper = root.join("overwrite");
        let work = root.join("work");
        let mount_path = paths::vfs_mount_path(workspace_root, instance_id, profile_id);
        for path in [&layer, &upper, &work, &mount_path] {
            fs::create_dir_all(path)?;
        }
        paths::guard_descendant(workspace_root, &mount_path.join(".guard"))?;

        if mount_is_active(&mount_path) {
            return Ok(status(
                instance_id,
                profile_id,
                mount_path,
                true,
                "Virtuelles Profil ist aktiv.",
            ));
        }
        self.mounts.remove(&key);
        let options = format!(
            "lowerdir={}:{},upperdir={},workdir={},auto_unmount",
            escape_overlay_path(&layer),
            escape_overlay_path(game_root),
            escape_overlay_path(&upper),
            escape_overlay_path(&work)
        );
        let mut child = Command::new("fuse-overlayfs")
            .args(["-o", &options])
            .arg(&mount_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                SlimError::Process(format!("fuse-overlayfs could not start: {error}"))
            })?;

        for _ in 0..50 {
            if mount_is_active(&mount_path) {
                self.mounts.insert(key, child);
                return Ok(status(
                    instance_id,
                    profile_id,
                    mount_path,
                    true,
                    "Virtuelles Profil wurde eingehängt.",
                ));
            }
            if let Some(exit) = child.try_wait()? {
                let stderr = child
                    .stderr
                    .take()
                    .and_then(|mut pipe| {
                        use std::io::Read;
                        let mut text = String::new();
                        pipe.read_to_string(&mut text).ok()?;
                        Some(text)
                    })
                    .unwrap_or_default();
                return Err(SlimError::Process(format!(
                    "fuse-overlayfs exited with {exit}: {}",
                    stderr.trim()
                )));
            }
            thread::sleep(Duration::from_millis(40));
        }
        let _ = child.kill();
        Err(SlimError::Process("fuse-overlayfs mount timed out".into()))
    }

    pub fn unmount(
        &mut self,
        workspace_root: &Path,
        instance_id: &str,
        profile_id: &str,
    ) -> SlimResult<VfsStatus> {
        let key = mount_key(instance_id, profile_id);
        let mount_path = paths::vfs_mount_path(workspace_root, instance_id, profile_id);
        if mount_is_active(&mount_path) {
            let result = Command::new("fusermount3")
                .args(["-u", "--"])
                .arg(&mount_path)
                .status()?;
            if !result.success() {
                return Err(SlimError::Process(format!(
                    "could not unmount {}",
                    mount_path.display()
                )));
            }
        }
        if let Some(mut child) = self.mounts.remove(&key) {
            let _ = child.wait();
        }
        Ok(status(
            instance_id,
            profile_id,
            mount_path,
            false,
            "Virtuelles Profil wurde ausgehängt.",
        ))
    }

    pub fn status(&self, workspace_root: &Path, instance_id: &str, profile_id: &str) -> VfsStatus {
        let mount_path = paths::vfs_mount_path(workspace_root, instance_id, profile_id);
        let available = command_exists("fuse-overlayfs") && command_exists("fusermount3");
        VfsStatus {
            available,
            mounted: mount_is_active(&mount_path),
            instance_id: instance_id.into(),
            profile_id: profile_id.into(),
            mount_path,
            message: if available {
                "fuse-overlayfs ist verfügbar.".into()
            } else {
                "fuse-overlayfs und fusermount3 werden benötigt.".into()
            },
        }
    }
}

impl Drop for VfsManager {
    fn drop(&mut self) {
        for (_, mut child) in std::mem::take(&mut self.mounts) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn ensure_linux_vfs_available() -> SlimResult<()> {
    if !cfg!(target_os = "linux") {
        return Err(SlimError::Disabled("SLiM-CC v2 VFS is Linux-only".into()));
    }
    if !command_exists("fuse-overlayfs") || !command_exists("fusermount3") {
        return Err(SlimError::Disabled(
            "fuse-overlayfs and fusermount3 are required".into(),
        ));
    }
    Ok(())
}
fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {command} >/dev/null 2>&1")])
        .status()
        .is_ok_and(|status| status.success())
}
fn mount_key(instance_id: &str, profile_id: &str) -> String {
    format!("{instance_id}:{profile_id}")
}
fn escape_overlay_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace(',', "\\,")
}
fn mount_is_active(path: &Path) -> bool {
    let needle = path.to_string_lossy().replace(' ', "\\040");
    fs::read_to_string("/proc/self/mountinfo").is_ok_and(|content| {
        content
            .lines()
            .any(|line| line.split_whitespace().nth(4) == Some(needle.as_ref()))
    })
}
fn status(
    instance_id: &str,
    profile_id: &str,
    mount_path: PathBuf,
    mounted: bool,
    message: &str,
) -> VfsStatus {
    VfsStatus {
        available: true,
        mounted,
        instance_id: instance_id.into(),
        profile_id: profile_id.into(),
        mount_path,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_paths_escape_option_separators() {
        assert_eq!(
            escape_overlay_path(Path::new("/tmp/a:b,c")),
            "/tmp/a\\:b\\,c"
        );
    }
    #[test]
    fn mount_keys_are_profile_specific() {
        assert_ne!(mount_key("game", "a"), mount_key("game", "b"));
    }
}
