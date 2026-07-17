use crate::error::{SlimError, SlimResult};
use crate::models::ModFile;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const GAME_ROOT_PREFIX: &str = "__slimcc_game_root__/";

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
            if ["esm", "esp", "esl", "bsa", "ba2"].contains(&ext.as_str()) {
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
    let data_root = match detect_data_root(mod_root) {
        Ok(root) => root,
        Err(error) => {
            if let Some(game_root) = find_vortex_game_root(mod_root)? {
                return scan_game_root_files(mod_id, &game_root);
            }
            return Err(error);
        }
    };
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

    if data_root
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("Data"))
    {
        if let Some(game_root) = data_root.parent() {
            for entry in std::fs::read_dir(game_root)? {
                let entry = entry?;
                if !entry.file_type()?.is_file() || !is_game_root_binary(&entry.path()) {
                    continue;
                }
                push_game_root_file(mod_id, game_root, entry.path(), &mut files)?;
            }
        }
    }

    Ok(files)
}

fn find_vortex_game_root(mod_root: &Path) -> SlimResult<Option<PathBuf>> {
    for entry in WalkDir::new(mod_root).follow_links(false).max_depth(4) {
        let entry = entry.map_err(|error| SlimError::InvalidPath(error.to_string()))?;
        if entry.file_type().is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("vortex_override_instructions.json")
        {
            return Ok(entry.path().parent().map(Path::to_path_buf));
        }
    }
    Ok(None)
}

fn scan_game_root_files(mod_id: &str, game_root: &Path) -> SlimResult<Vec<ModFile>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(game_root).follow_links(false) {
        let entry = entry.map_err(|error| SlimError::InvalidPath(error.to_string()))?;
        if !entry.file_type().is_file()
            || entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("vortex_override_instructions.json")
        {
            continue;
        }
        push_game_root_file(mod_id, game_root, entry.path().to_path_buf(), &mut files)?;
    }
    Ok(files)
}

fn push_game_root_file(
    mod_id: &str,
    game_root: &Path,
    path: PathBuf,
    files: &mut Vec<ModFile>,
) -> SlimResult<()> {
    let rel = path
        .strip_prefix(game_root)
        .map_err(|_| SlimError::InvalidPath("failed to strip game root".into()))?;
    let original_rel_path = format!(
        "{GAME_ROOT_PREFIX}{}",
        rel.to_string_lossy().replace('\\', "/")
    );
    let normalized_rel_path = normalize_rel_path(&original_rel_path);
    let file_size = path.metadata().ok().map(|metadata| metadata.len());
    files.push(ModFile {
        mod_id: mod_id.to_string(),
        original_rel_path,
        normalized_rel_path,
        abs_source_path: path,
        file_size,
    });
    Ok(())
}

fn is_game_root_binary(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("dll") || extension.eq_ignore_ascii_case("exe")
        })
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

    #[test]
    fn detect_data_root_accepts_archive_only_mods() {
        let root = temp_root("archive-only-mod");
        std::fs::write(root.join("aMidianBorn_ContentAddon.bsa"), b"archive").expect("write BSA");

        assert_eq!(detect_data_root(&root).expect("detect BSA root"), root);
        let files = scan_mod_files("amidian", &root).expect("scan BSA-only mod");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].original_rel_path, "aMidianBorn_ContentAddon.bsa");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn scanner_marks_vortex_root_installer_files_for_the_game_layer() {
        let root = temp_root("vortex-game-root");
        std::fs::write(root.join("vortex_override_instructions.json"), b"[]")
            .expect("write Vortex instructions");
        std::fs::write(root.join("d3dx9_42.dll"), b"preloader").expect("write root DLL");

        let files = scan_mod_files("root-mod", &root).expect("scan game-root installer");

        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].original_rel_path,
            "__slimcc_game_root__/d3dx9_42.dll"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn scanner_keeps_skse_data_and_root_binaries_in_separate_scopes() {
        let root = temp_root("skse-mixed-root");
        let wrapper = root.join("skse64_2_02_06");
        std::fs::create_dir_all(wrapper.join("Data/Scripts")).expect("create Data tree");
        std::fs::write(wrapper.join("Data/Scripts/example.pex"), b"script").expect("write script");
        std::fs::write(wrapper.join("skse64_loader.exe"), b"loader").expect("write loader");
        std::fs::write(wrapper.join("skse64_runtime.dll"), b"runtime").expect("write runtime");

        let files = scan_mod_files("skse", &root).expect("scan mixed SKSE archive");
        let paths = files
            .iter()
            .map(|file| file.original_rel_path.as_str())
            .collect::<Vec<_>>();

        assert!(paths.contains(&"Scripts/example.pex"), "paths: {paths:?}");
        assert!(paths.contains(&"__slimcc_game_root__/skse64_loader.exe"));
        assert!(paths.contains(&"__slimcc_game_root__/skse64_runtime.dll"));
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
