use crate::error::{SlimError, SlimResult};
use crate::models::ModFile;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn normalize_rel_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

pub fn detect_data_root(mod_root: &Path) -> SlimResult<PathBuf> {
    if let Some(root) = direct_data_folder(mod_root) {
        return Ok(root);
    }

    if looks_like_data_root(mod_root)? {
        return Ok(mod_root.to_path_buf());
    }

    // Nexus archives often contain one or two wrapper folders before Data contents.
    // Keep this shallow so unrelated docs/screenshots folders are not treated as mods.
    for entry in WalkDir::new(mod_root)
        .follow_links(false)
        .min_depth(1)
        .max_depth(6)
    {
        let entry = entry.map_err(|err| SlimError::InvalidPath(err.to_string()))?;
        if !entry.file_type().is_dir() {
            continue;
        }
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case("Data")
        {
            return Ok(entry.path().to_path_buf());
        }
    }

    for entry in WalkDir::new(mod_root)
        .follow_links(false)
        .min_depth(1)
        .max_depth(6)
    {
        let entry = entry.map_err(|err| SlimError::InvalidPath(err.to_string()))?;
        if !entry.file_type().is_dir() {
            continue;
        }
        if looks_like_data_root(entry.path())? {
            return Ok(entry.path().to_path_buf());
        }
    }

    if looks_like_mod_root(mod_root)? {
        return Ok(mod_root.to_path_buf());
    }

    Err(SlimError::NotFound(format!(
        "could not detect Data root in {}",
        mod_root.display()
    )))
}

fn direct_data_folder(root: &Path) -> Option<PathBuf> {
    let direct_data = root.join("Data");
    direct_data.is_dir().then_some(direct_data)
}

fn looks_like_data_root(root: &Path) -> SlimResult<bool> {
    let markers = [
        "meshes",
        "textures",
        "scripts",
        "skse",
        "interface",
        "sound",
        "calientetools",
    ];
    for marker in markers {
        if has_child_dir_case_insensitive(root, marker)? {
            return Ok(true);
        }
    }

    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if let Some(ext) = entry.path().extension().and_then(|v| v.to_str()) {
            let ext = ext.to_lowercase();
            if ["esm", "esp", "esl"].contains(&ext.as_str()) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn looks_like_mod_root(root: &Path) -> SlimResult<bool> {
    let markers = ["meshes", "calientetools"];
    for marker in markers {
        if has_child_dir_case_insensitive(root, marker)? {
            return Ok(true);
        }
    }

    Ok(false)
}

fn has_child_dir_case_insensitive(root: &Path, name: &str) -> SlimResult<bool> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        {
            return Ok(true);
        }
    }

    Ok(false)
}

pub fn scan_mod_files(mod_id: &str, mod_root: &Path) -> SlimResult<Vec<ModFile>> {
    let data_root = detect_data_root(mod_root)?;
    let mut files = Vec::new();

    for entry in WalkDir::new(&data_root).follow_links(false) {
        let entry = entry.map_err(|err| SlimError::InvalidPath(err.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }

        let abs_source_path = entry.path().to_path_buf();
        let rel = abs_source_path
            .strip_prefix(&data_root)
            .map_err(|_| SlimError::InvalidPath("failed to strip Data root".into()))?;
        let original_rel_path = rel.to_string_lossy().replace('\\', "/");
        let normalized_rel_path = normalize_rel_path(&original_rel_path);
        let file_size = entry.metadata().ok().map(|m| m.len());

        files.push(ModFile {
            mod_id: mod_id.to_string(),
            original_rel_path,
            normalized_rel_path,
            abs_source_path,
            file_size,
        });
    }

    Ok(files)
}

pub fn is_plugin_path(rel_path: &str) -> bool {
    let lower = rel_path.to_lowercase();
    lower.ends_with(".esm") || lower.ends_with(".esp") || lower.ends_with(".esl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rel_path_lowercases_and_unifies_separators() {
        assert_eq!(
            normalize_rel_path(r"Meshes\Armor\Iron\IRON.NIF"),
            "meshes/armor/iron/iron.nif"
        );
    }

    #[test]
    fn plugin_detection_is_case_insensitive() {
        assert!(is_plugin_path("Plugins/Example.ESM"));
        assert!(is_plugin_path("Plugins/Example.ESP"));
        assert!(is_plugin_path("Plugins/Example.esl"));
        assert!(!is_plugin_path("Meshes/Example.nif"));
    }

    #[test]
    fn detect_data_root_accepts_archive_wrapper_with_data_folder() {
        let root = temp_root("wrapped-data-folder");
        let data = root.join("Wrapper").join("Data");
        std::fs::create_dir_all(data.join("SKSE/Plugins")).expect("create data tree");
        std::fs::write(data.join("SKSE/Plugins/example.dll"), b"").expect("write file");

        let detected = detect_data_root(&root).expect("detect wrapped Data folder");

        assert_eq!(detected, data);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn detect_data_root_accepts_archive_wrapper_with_direct_data_contents() {
        let root = temp_root("wrapped-direct-data");
        let wrapped = root.join("Wrapper");
        std::fs::create_dir_all(wrapped.join("meshes/armor")).expect("create wrapped meshes");
        std::fs::write(wrapped.join("meshes/armor/example.nif"), b"").expect("write file");

        let detected = detect_data_root(&root).expect("detect wrapped data contents");

        assert_eq!(detected, wrapped);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn detect_data_root_accepts_mod_root_with_uppercase_meshes_and_caliente_tools() {
        let root = temp_root("uppercase-wrapperless-mod");
        std::fs::create_dir_all(root.join("Meshes/armor")).expect("create meshes");
        std::fs::create_dir_all(root.join("CalienteTools/BodySlide"))
            .expect("create caliente tools");
        std::fs::write(root.join("Meshes/armor/example.nif"), b"").expect("write mesh");

        let detected = detect_data_root(&root).expect("detect mod root");

        assert_eq!(detected, root);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn detect_data_root_accepts_deeply_nested_wrapper_with_data_folder() {
        let root = temp_root("deeply-nested-wrapper");
        let nested = root
            .join("Wrapper")
            .join("Optional")
            .join("Main")
            .join("Data");
        std::fs::create_dir_all(nested.join("SKSE/Plugins")).expect("create nested data tree");
        std::fs::write(nested.join("SKSE/Plugins/example.dll"), b"").expect("write file");

        let detected = detect_data_root(&root).expect("detect deeply nested data folder");

        assert_eq!(detected, nested);
        let _ = std::fs::remove_dir_all(root);
    }

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("slim-cc-scanner-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }
}
