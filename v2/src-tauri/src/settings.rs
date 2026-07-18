use crate::error::SlimResult;
use crate::models::ModDownloadCandidate;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub const SETTING_INSTALL_PATH: &str = "default_install_path";
pub const SETTING_DATA_PATH: &str = "default_data_path";
pub const SETTING_WINE_PREFIX: &str = "default_wine_prefix";
pub const SETTING_LOOT_EXECUTABLE_PATH: &str = "loot_executable_path";
pub const SETTING_MOD_DOWNLOAD_PATH: &str = "mod_download_path";
pub const SETTING_LANGUAGE: &str = "language";
pub const SETTING_NEXUS_API_KEY: &str = "nexus_api_key";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub install_path: Option<PathBuf>,
    pub data_path: Option<PathBuf>,
    pub wine_prefix: Option<PathBuf>,
    pub loot_executable_path: Option<PathBuf>,
    pub mod_download_path: Option<PathBuf>,
    pub language: Option<String>,
    pub nexus_api_key_configured: bool,
    pub nexus_api_key_masked: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAppSettingsRequest {
    pub install_path: Option<PathBuf>,
    pub data_path: Option<PathBuf>,
    pub wine_prefix: Option<PathBuf>,
    pub loot_executable_path: Option<PathBuf>,
    pub mod_download_path: Option<PathBuf>,
    pub language: Option<String>,
    pub nexus_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNexusApiKeyRequest {
    pub nexus_api_key: String,
}

pub fn load_settings(conn: &Connection) -> SlimResult<AppSettings> {
    let nexus_api_key = load_nexus_api_key(conn)?;
    Ok(AppSettings {
        install_path: get_path(conn, SETTING_INSTALL_PATH)?,
        data_path: get_path(conn, SETTING_DATA_PATH)?,
        wine_prefix: get_path(conn, SETTING_WINE_PREFIX)?,
        loot_executable_path: get_path(conn, SETTING_LOOT_EXECUTABLE_PATH)?,
        mod_download_path: get_path(conn, SETTING_MOD_DOWNLOAD_PATH)?,
        language: get_text(conn, SETTING_LANGUAGE)?,
        nexus_api_key_configured: nexus_api_key.is_some(),
        nexus_api_key_masked: nexus_api_key.as_deref().map(mask_api_key),
    })
}

pub fn save_settings(
    conn: &mut Connection,
    request: UpdateAppSettingsRequest,
) -> SlimResult<AppSettings> {
    if let Some(install_path) = request.install_path.as_deref() {
        crate::instance::validate_wine_prefix(install_path, request.wine_prefix.as_deref())?;
    }
    let tx = conn.transaction()?;
    set_path(&tx, SETTING_INSTALL_PATH, request.install_path.clone())?;
    set_path(&tx, SETTING_DATA_PATH, request.data_path.clone())?;
    set_path(&tx, SETTING_WINE_PREFIX, request.wine_prefix.clone())?;
    set_path(
        &tx,
        SETTING_LOOT_EXECUTABLE_PATH,
        request.loot_executable_path.clone(),
    )?;
    set_path(
        &tx,
        SETTING_MOD_DOWNLOAD_PATH,
        request.mod_download_path.clone(),
    )?;
    set_text(&tx, SETTING_LANGUAGE, request.language.clone())?;
    match request.nexus_api_key.as_deref() {
        Some(raw) => set_nexus_api_key(&tx, raw)?,
        None => {}
    }
    tx.commit()?;
    load_settings(conn)
}

pub fn save_nexus_api_key(
    conn: &mut Connection,
    request: UpdateNexusApiKeyRequest,
) -> SlimResult<AppSettings> {
    let tx = conn.transaction()?;
    set_nexus_api_key(&tx, &request.nexus_api_key)?;
    tx.commit()?;
    load_settings(conn)
}

pub fn load_nexus_api_key(conn: &Connection) -> SlimResult<Option<String>> {
    Ok(get_text(conn, SETTING_NEXUS_API_KEY)?.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        if trimmed.contains('\n') {
            parse_api_key(trimmed)
        } else {
            Some(trimmed.to_string())
        }
    }))
}

fn set_nexus_api_key(conn: &Connection, raw: &str) -> SlimResult<()> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return set_text(conn, SETTING_NEXUS_API_KEY, None);
    }

    let stored = if trimmed.contains('\n') {
        parse_api_key(trimmed).unwrap_or_else(|| trimmed.to_string())
    } else {
        trimmed.to_string()
    };
    set_text(conn, SETTING_NEXUS_API_KEY, Some(stored))
}

pub fn parse_api_key(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        let candidate = trimmed
            .split_once([':', '='])
            .map(|(_, value)| value)
            .unwrap_or(trimmed)
            .trim()
            .trim_matches(|ch| matches!(ch, '"' | '\'' | '`'));
        (!candidate.is_empty()).then(|| candidate.to_string())
    })
}

pub fn mask_api_key(api_key: &str) -> String {
    let chars: Vec<char> = api_key.chars().collect();
    if chars.len() <= 4 {
        return "*".repeat(chars.len().max(1));
    }
    let start: String = chars.iter().take(2).collect();
    let end: String = chars
        .iter()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{start}**********{end}")
}

pub fn list_mod_download_candidates(
    conn: &Connection,
    instance_id: Option<&str>,
) -> SlimResult<Vec<ModDownloadCandidate>> {
    let Some(download_path) = load_settings(conn)?.mod_download_path else {
        return Ok(Vec::new());
    };
    if !download_path.exists() {
        return Ok(Vec::new());
    }

    let installed_candidates = load_installed_candidate_index(conn, instance_id)?;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(download_path)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let installed =
                path_matches_installed_candidate(&path, &installed_candidates, &file_name);
            candidates.push(ModDownloadCandidate {
                name: file_name,
                path,
                entry_type: "folder".into(),
                importable: true,
                installed,
                note: None,
            });
        } else if file_type.is_file() && is_known_archive(&path) {
            let stable = !file_name.to_ascii_lowercase().ends_with(".part")
                && !file_name.to_ascii_lowercase().ends_with(".tmp")
                && !file_name.to_ascii_lowercase().ends_with(".crdownload");
            let importable = is_extractable_archive(&path) && stable;
            let installed = path_matches_installed_candidate(
                &path,
                &installed_candidates,
                &path
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or(file_name.clone()),
            );
            candidates.push(ModDownloadCandidate {
                name: path
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or(file_name),
                path,
                entry_type: "archive".into(),
                importable,
                installed,
                note: if !stable {
                    Some("Download läuft möglicherweise noch; bitte kurz warten.".into())
                } else if importable {
                    None
                } else {
                    Some("This archive type is not supported yet.".into())
                },
            });
        }
    }

    candidates.sort_by(|a, b| {
        b.importable
            .cmp(&a.importable)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(candidates)
}

struct InstalledCandidateIndex {
    paths: HashSet<PathBuf>,
    names: HashSet<String>,
}

fn load_installed_candidate_index(
    conn: &Connection,
    instance_id: Option<&str>,
) -> SlimResult<InstalledCandidateIndex> {
    let sql = if instance_id.is_some() {
        "SELECT name, source_path, installed_path FROM mods WHERE instance_id=?1"
    } else {
        "SELECT name, source_path, installed_path FROM mods WHERE ?1 IS NULL"
    };
    let mut statement = conn.prepare(sql)?;
    let mut rows = statement.query(params![instance_id])?;
    let mut paths = HashSet::new();
    let mut names = HashSet::new();
    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        let source_path: String = row.get(1)?;
        let installed_path: String = row.get(2)?;
        insert_candidate_name(&mut names, &name);
        insert_candidate_path(&mut names, Path::new(&source_path));
        insert_candidate_path(&mut names, Path::new(&installed_path));
        paths.insert(PathBuf::from(source_path));
        paths.insert(PathBuf::from(installed_path));
    }
    Ok(InstalledCandidateIndex { paths, names })
}

fn path_matches_installed_candidate(
    candidate_path: &Path,
    installed_candidates: &InstalledCandidateIndex,
    candidate_name: &str,
) -> bool {
    if installed_candidates.paths.contains(candidate_path) {
        return true;
    }

    let candidate_name = candidate_name.trim().to_lowercase();
    if installed_candidates.names.contains(&candidate_name) {
        return true;
    }

    false
}

fn insert_candidate_path(names: &mut HashSet<String>, path: &Path) {
    if let Some(name) = path.file_stem().and_then(|value| value.to_str()) {
        insert_candidate_name(names, name);
    }
    if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
        insert_candidate_name(names, name);
    }
}

fn insert_candidate_name(names: &mut HashSet<String>, name: &str) {
    let exact = name.trim().to_lowercase();
    if !exact.is_empty() {
        names.insert(exact);
    }
}

fn archive_extension(path: &Path) -> Option<String> {
    path.extension()
        .map(|value| value.to_string_lossy().to_lowercase())
}

fn is_known_archive(path: &Path) -> bool {
    let Some(extension) = archive_extension(path) else {
        return false;
    };
    matches!(
        extension.as_str(),
        "7z" | "zip" | "rar" | "fomod" | "omod" | "tar" | "gz" | "bz2" | "xz"
    )
}

fn is_extractable_archive(path: &Path) -> bool {
    let Some(extension) = archive_extension(path) else {
        return false;
    };
    matches!(extension.as_str(), "7z" | "zip" | "rar" | "fomod" | "omod")
}

fn get_path(conn: &Connection, key: &str) -> SlimResult<Option<PathBuf>> {
    let value = conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    );

    match value {
        Ok(path) => Ok(Some(PathBuf::from(path))),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn set_path(conn: &Connection, key: &str, value: Option<PathBuf>) -> SlimResult<()> {
    match value {
        Some(path) => {
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, path.to_string_lossy().to_string()],
            )?;
        }
        None => {
            conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])?;
        }
    }

    Ok(())
}

fn get_text(conn: &Connection, key: &str) -> SlimResult<Option<String>> {
    let value = conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    );

    match value {
        Ok(text) => Ok(Some(text)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn set_text(conn: &Connection, key: &str, value: Option<String>) -> SlimResult<()> {
    match value {
        Some(text) => {
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, text],
            )?;
        }
        None => {
            conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("slimcc-{name}-{}", Uuid::new_v4()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn create_settings_schema(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
             CREATE TABLE mods (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                source_path TEXT NOT NULL,
                installed_path TEXT NOT NULL
            );",
        )
        .expect("create settings schema");
    }

    #[test]
    fn save_settings_stores_nexus_key_but_loads_only_masked_display() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);

        let settings = save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: None,
                language: Some("en".into()),
                nexus_api_key: Some("abcd1234wxyz".into()),
            },
        )
        .expect("save settings");

        assert!(settings.nexus_api_key_configured);
        assert_eq!(
            settings.nexus_api_key_masked.as_deref(),
            Some("ab**********yz")
        );
        assert_eq!(
            load_nexus_api_key(&conn).unwrap().as_deref(),
            Some("abcd1234wxyz")
        );
    }

    #[test]
    fn save_settings_preserves_raw_nexus_key_with_equals_signs() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);

        let raw_key = "abc/123==";
        let settings = save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: None,
                language: Some("en".into()),
                nexus_api_key: Some(raw_key.into()),
            },
        )
        .expect("save settings");

        assert!(settings.nexus_api_key_configured);
        assert_eq!(load_nexus_api_key(&conn).unwrap().as_deref(), Some(raw_key));
    }

    #[test]
    fn save_settings_clears_nexus_key_when_input_is_blank() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);

        save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: None,
                language: Some("en".into()),
                nexus_api_key: Some("abcd1234wxyz".into()),
            },
        )
        .expect("save settings");

        save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: None,
                language: Some("en".into()),
                nexus_api_key: Some("   ".into()),
            },
        )
        .expect("clear settings");

        assert_eq!(load_nexus_api_key(&conn).unwrap(), None);
    }

    #[test]
    fn save_nexus_api_key_does_not_require_game_paths() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);

        let settings = save_nexus_api_key(
            &mut conn,
            UpdateNexusApiKeyRequest {
                nexus_api_key: "abcd1234wxyz".into(),
            },
        )
        .expect("save Nexus API key");

        assert!(settings.nexus_api_key_configured);
        assert_eq!(settings.install_path, None);
        assert_eq!(settings.data_path, None);
        assert_eq!(
            load_nexus_api_key(&conn).unwrap().as_deref(),
            Some("abcd1234wxyz")
        );
    }

    #[test]
    fn save_settings_without_nexus_api_key_preserves_existing_key() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);

        save_nexus_api_key(
            &mut conn,
            UpdateNexusApiKeyRequest {
                nexus_api_key: "abcd1234wxyz".into(),
            },
        )
        .expect("save Nexus API key");

        let settings = save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: None,
                language: Some("en".into()),
                nexus_api_key: None,
            },
        )
        .expect("save settings");

        assert!(settings.nexus_api_key_configured);
        assert_eq!(
            load_nexus_api_key(&conn).unwrap().as_deref(),
            Some("abcd1234wxyz")
        );
    }

    #[test]
    fn list_mod_download_candidates_returns_importable_folders_and_archives() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);
        let download_path = temp_dir("download-candidates");
        fs::create_dir_all(download_path.join("SkyUI")).expect("create folder mod");
        fs::write(download_path.join("Address Library.zip"), b"archive").expect("write archive");
        fs::write(download_path.join("notes.txt"), b"ignore").expect("write note");

        save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: Some(download_path.clone()),
                language: Some("en".into()),
                nexus_api_key: None,
            },
        )
        .expect("save settings");

        let candidates = list_mod_download_candidates(&conn, None).expect("list candidates");

        assert_eq!(candidates.len(), 2);
        let folder = candidates
            .iter()
            .find(|candidate| candidate.name == "SkyUI")
            .expect("folder candidate");
        assert!(folder.importable);
        assert_eq!(folder.entry_type, "folder");
        let archive = candidates
            .iter()
            .find(|candidate| candidate.name == "Address Library")
            .expect("archive candidate");
        assert!(archive.importable);
        assert_eq!(archive.entry_type, "archive");

        let _ = fs::remove_dir_all(download_path);
    }

    #[test]
    fn list_mod_download_candidates_marks_matching_archives_as_installed_by_mod_name() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);
        let download_path = temp_dir("download-installed-archive");
        fs::write(download_path.join("Address Library.zip"), b"archive").expect("write archive");

        save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: Some(download_path.clone()),
                language: Some("en".into()),
                nexus_api_key: None,
            },
        )
        .expect("save settings");

        conn.execute(
            "INSERT INTO mods (id, name, source_path, installed_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                "mod-1",
                "Address Library",
                "/workspace/instance/mods/mod-1-source",
                "/workspace/instance/mods/mod-1"
            ],
        )
        .expect("insert mod");

        let candidates = list_mod_download_candidates(&conn, None).expect("list candidates");
        let archive = candidates
            .iter()
            .find(|candidate| candidate.name == "Address Library")
            .expect("archive candidate");
        assert!(archive.installed);

        let _ = fs::remove_dir_all(download_path);
    }

    #[test]
    fn list_mod_download_candidates_marks_nexus_suffixed_fomod_as_installed() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);
        let download_path = temp_dir("download-installed-fomod-suffix");
        fs::write(
            download_path.join("Afterlife - Resurrected-55051-1-0-1700000000.zip"),
            b"archive",
        )
        .expect("write archive");

        save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: Some(download_path.clone()),
                language: Some("en".into()),
                nexus_api_key: None,
            },
        )
        .expect("save settings");

        conn.execute(
            "INSERT INTO mods (id, name, source_path, installed_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                "mod-1",
                "Afterlife - Resurrected",
                "/workspace/fomod-preview/old-afterlife-preview",
                "/workspace/instance/mods/mod-1"
            ],
        )
        .expect("insert mod");

        let candidates = list_mod_download_candidates(&conn, None).expect("list candidates");
        let archive = candidates
            .iter()
            .find(|candidate| candidate.name.starts_with("Afterlife"))
            .expect("archive candidate");
        assert!(
            !archive.installed,
            "ambiguous suffixed names must not be treated as installed"
        );

        let _ = fs::remove_dir_all(download_path);
    }

    #[test]
    fn list_mod_download_candidates_marks_nexus_prefixed_fomod_as_installed() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        create_settings_schema(&conn);
        let download_path = temp_dir("download-installed-fomod-prefix");
        fs::write(
            download_path.join("55051 - Afterlife - Resurrected.zip"),
            b"archive",
        )
        .expect("write archive");

        save_settings(
            &mut conn,
            UpdateAppSettingsRequest {
                install_path: Some(PathBuf::from("/game")),
                data_path: Some(PathBuf::from("/game/Data")),
                wine_prefix: None,
                loot_executable_path: None,
                mod_download_path: Some(download_path.clone()),
                language: Some("en".into()),
                nexus_api_key: None,
            },
        )
        .expect("save settings");

        conn.execute(
            "INSERT INTO mods (id, name, source_path, installed_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                "mod-1",
                "Afterlife - Resurrected",
                "/workspace/instance/mods/mod-1-source",
                "/workspace/instance/mods/mod-1"
            ],
        )
        .expect("insert mod");

        let candidates = list_mod_download_candidates(&conn, None).expect("list candidates");
        let archive = candidates
            .iter()
            .find(|candidate| candidate.name.contains("Afterlife"))
            .expect("archive candidate");
        assert!(
            !archive.installed,
            "ambiguous prefixed names must not be treated as installed"
        );

        let _ = fs::remove_dir_all(download_path);
    }
}
