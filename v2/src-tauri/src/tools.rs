use crate::error::{SlimError, SlimResult};
use crate::models::{ToolExecutionResult, ToolProfileValidation, ToolRunRecord};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Child, Command};
use uuid::Uuid;

const TOOL_PROFILE_SCHEMA: &str = include_str!("../migrations/003_tool_profiles.sql");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunnerType {
    Native,
    Wine,
    Proton,
    Lutris,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolLaunchRequest {
    pub runner_type: RunnerType,
    pub executable_path: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub wine_prefix: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPreview {
    pub program: String,
    pub args: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub process_id: Option<u32>,
    pub execution: Option<ToolExecutionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProfile {
    pub id: String,
    pub tool_key: String,
    pub display_name: String,
    pub executable_path: Option<PathBuf>,
    pub runner_type: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub wine_prefix: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateToolProfileRequest {
    pub tool_key: String,
    pub display_name: String,
    pub executable_path: Option<PathBuf>,
    pub runner_type: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub wine_prefix: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchToolProfileRequest {
    pub tool_key: String,
}

pub fn initialize_tool_profile_schema(conn: &Connection) -> SlimResult<()> {
    conn.execute_batch(TOOL_PROFILE_SCHEMA)?;
    Ok(())
}

pub fn list_tool_profiles(conn: &Connection) -> SlimResult<Vec<ToolProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, tool_key, display_name, executable_path, runner_type, arguments_json,
                working_directory, wine_prefix, log_path, enabled
         FROM tool_profiles
         ORDER BY CASE tool_key
             WHEN 'loot' THEN 1
             WHEN 'xedit' THEN 2
             WHEN 'nemesis' THEN 3
             WHEN 'fnis' THEN 4
             WHEN 'pandora' THEN 5
             WHEN 'bodyslide' THEN 6
             ELSE 99
         END, display_name ASC",
    )?;
    let rows = stmt.query_map([], row_to_tool_profile)?;

    let mut profiles = Vec::new();
    for row in rows {
        profiles.push(row?);
    }
    Ok(profiles)
}

pub fn validate_tool_profiles(conn: &Connection) -> SlimResult<Vec<ToolProfileValidation>> {
    let settings = crate::settings::load_settings(conn)?;
    let profiles = list_tool_profiles(conn)?;
    Ok(profiles
        .into_iter()
        .map(|profile| validate_tool_profile(profile, &settings))
        .collect())
}

fn validate_tool_profile(
    profile: ToolProfile,
    settings: &crate::settings::AppSettings,
) -> ToolProfileValidation {
    let executable_configured = profile.executable_path.is_some();
    let executable_exists = profile
        .executable_path
        .as_ref()
        .map(|path| path.is_file())
        .unwrap_or(false);
    let effective_working_directory = profile
        .working_directory
        .clone()
        .or(settings.install_path.clone());
    let effective_wine_prefix = profile.wine_prefix.clone().or(settings.wine_prefix.clone());
    let effective_log_path = profile
        .log_path
        .clone()
        .or_else(|| default_tool_log_path(&profile.tool_key, effective_working_directory.as_ref()));
    let working_directory_exists = effective_working_directory
        .as_ref()
        .map(|path| path.is_dir())
        .unwrap_or(false);
    let wine_prefix_exists = if profile.runner_type.eq_ignore_ascii_case("wine") {
        effective_wine_prefix
            .as_ref()
            .map(|path| path.is_dir())
            .unwrap_or(false)
    } else {
        true
    };

    let mut notes = Vec::new();
    if !profile.enabled {
        notes.push("Tool profile is disabled.".into());
    }
    if !executable_configured {
        notes.push("Executable path is not configured.".into());
    } else if !executable_exists {
        notes.push("Executable path does not exist or is not a file.".into());
    }
    if !working_directory_exists {
        notes.push("Effective working directory is missing.".into());
    }
    if !wine_prefix_exists {
        notes.push("Effective Wine prefix is missing.".into());
    }
    if profile.log_path.is_none() {
        if let Some(path) = &effective_log_path {
            notes.push(format!("Log path defaults to {}.", path.display()));
        }
    }
    if matches!(profile.runner_type.as_str(), "proton" | "lutris") {
        notes.push("Runner is declared but not executable in this build.".into());
    }

    let status = if !profile.enabled {
        "disabled"
    } else if executable_exists && working_directory_exists && wine_prefix_exists {
        "ok"
    } else {
        "needs_attention"
    };

    ToolProfileValidation {
        tool_key: profile.tool_key,
        display_name: profile.display_name,
        enabled: profile.enabled,
        executable_configured,
        executable_exists,
        working_directory_exists,
        wine_prefix_exists,
        effective_log_path,
        status: status.into(),
        notes,
    }
}

pub fn get_tool_profile(conn: &Connection, tool_key: &str) -> SlimResult<ToolProfile> {
    conn.query_row(
        "SELECT id, tool_key, display_name, executable_path, runner_type, arguments_json,
                working_directory, wine_prefix, log_path, enabled
         FROM tool_profiles
         WHERE tool_key = ?1",
        params![tool_key],
        row_to_tool_profile,
    )
    .map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => {
            SlimError::NotFound(format!("tool profile not found: {tool_key}"))
        }
        other => other.into(),
    })
}

pub fn upsert_tool_profile(
    conn: &mut Connection,
    request: UpdateToolProfileRequest,
) -> SlimResult<ToolProfile> {
    let runner_type = normalize_runner_type(&request.runner_type)?;
    let arguments_json = serde_json::to_string(&request.arguments)?;
    let now = Utc::now().to_rfc3339();
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM tool_profiles WHERE tool_key = ?1",
            params![&request.tool_key],
            |row| row.get(0),
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    let id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    conn.execute(
        "INSERT INTO tool_profiles (
             id, tool_key, display_name, executable_path, runner_type, arguments_json,
             working_directory, wine_prefix, log_path, enabled, created_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(tool_key) DO UPDATE SET
             display_name = excluded.display_name,
             executable_path = excluded.executable_path,
             runner_type = excluded.runner_type,
             arguments_json = excluded.arguments_json,
             working_directory = excluded.working_directory,
             wine_prefix = excluded.wine_prefix,
             log_path = excluded.log_path,
             enabled = excluded.enabled,
             updated_at = excluded.updated_at",
        params![
            id,
            request.tool_key,
            request.display_name,
            path_to_string(request.executable_path.as_ref()),
            runner_type,
            arguments_json,
            path_to_string(request.working_directory.as_ref()),
            path_to_string(request.wine_prefix.as_ref()),
            path_to_string(request.log_path.as_ref()),
            if request.enabled { 1 } else { 0 },
            now
        ],
    )?;

    get_tool_profile(conn, &request.tool_key)
}

pub fn launch_tool_profile(
    conn: &mut Connection,
    request: LaunchToolProfileRequest,
) -> SlimResult<CommandPreview> {
    let profile = get_tool_profile(conn, &request.tool_key)?;
    if !profile.enabled {
        return Err(SlimError::Disabled(format!(
            "tool profile is disabled: {}",
            profile.display_name
        )));
    }
    let executable_path = profile.executable_path.clone().ok_or_else(|| {
        SlimError::InvalidPath(format!(
            "tool executable is not configured: {}",
            profile.display_name
        ))
    })?;
    if !executable_path.exists() {
        return Err(SlimError::NotFound(format!(
            "tool executable not found: {}",
            executable_path.display()
        )));
    }

    let settings = crate::settings::load_settings(conn)?;
    let launch_request = build_tool_launch_request(&profile, executable_path, &settings)?;
    let mut preview = build_command_preview(&launch_request)?;
    let started_at = Utc::now().to_rfc3339();
    let (process_id, execution) = launch_tool(&launch_request)?;
    let finished_at = Utc::now().to_rfc3339();
    record_tool_run(
        conn,
        &profile.tool_key,
        &profile.display_name,
        &preview,
        &execution,
        &started_at,
        &finished_at,
    )?;
    preview.process_id = Some(process_id);
    preview.execution = Some(execution);
    Ok(preview)
}

pub fn record_tool_run(
    conn: &Connection,
    tool_key: &str,
    display_name: &str,
    preview: &CommandPreview,
    execution: &ToolExecutionResult,
    started_at: &str,
    finished_at: &str,
) -> SlimResult<ToolRunRecord> {
    let id = Uuid::new_v4().to_string();
    let arguments_json = serde_json::to_string(&preview.args)?;
    conn.execute(
        "INSERT INTO tool_runs (
             id, tool_key, display_name, program, arguments_json, working_directory,
             exit_code, stdout, stderr, started_at, finished_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            &id,
            tool_key,
            display_name,
            &preview.program,
            arguments_json,
            path_to_string(preview.working_directory.as_ref()),
            execution.exit_code,
            execution.stdout,
            execution.stderr,
            started_at,
            finished_at
        ],
    )?;
    get_tool_run(conn, &id)
}

pub fn list_recent_tool_runs(conn: &Connection, limit: i64) -> SlimResult<Vec<ToolRunRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, tool_key, display_name, program, arguments_json, working_directory,
                exit_code, stdout, stderr, started_at, finished_at
         FROM tool_runs
         ORDER BY finished_at DESC, id DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit.max(1)], row_to_tool_run)?;
    let mut runs = Vec::new();
    for row in rows {
        runs.push(row?);
    }
    Ok(runs)
}

fn get_tool_run(conn: &Connection, id: &str) -> SlimResult<ToolRunRecord> {
    Ok(conn.query_row(
        "SELECT id, tool_key, display_name, program, arguments_json, working_directory,
                exit_code, stdout, stderr, started_at, finished_at
         FROM tool_runs
         WHERE id = ?1",
        params![id],
        row_to_tool_run,
    )?)
}

pub fn build_tool_launch_request(
    profile: &ToolProfile,
    executable_path: PathBuf,
    settings: &crate::settings::AppSettings,
) -> SlimResult<ToolLaunchRequest> {
    build_tool_launch_request_with_arguments(
        profile,
        executable_path,
        settings,
        profile.arguments.clone(),
    )
}

pub fn build_tool_launch_request_with_arguments(
    profile: &ToolProfile,
    executable_path: PathBuf,
    settings: &crate::settings::AppSettings,
    arguments: Vec<String>,
) -> SlimResult<ToolLaunchRequest> {
    let arguments = tool_specific_arguments(&profile.tool_key, arguments);
    let working_directory = profile.working_directory.clone().or_else(|| {
        if matches!(
            profile.tool_key.to_ascii_lowercase().as_str(),
            "nemesis" | "fnis" | "pandora" | "bodyslide"
        ) {
            executable_path.parent().map(PathBuf::from)
        } else {
            settings.install_path.clone()
        }
    });

    Ok(ToolLaunchRequest {
        runner_type: runner_type_from_label(&profile.runner_type)?,
        executable_path,
        arguments,
        working_directory,
        wine_prefix: profile.wine_prefix.clone().or(settings.wine_prefix.clone()),
    })
}

fn tool_specific_arguments(tool_key: &str, mut arguments: Vec<String>) -> Vec<String> {
    if tool_key.eq_ignore_ascii_case("xedit")
        && !arguments.iter().any(|arg| arg.eq_ignore_ascii_case("-sse"))
    {
        arguments.insert(0, "-SSE".into());
    }
    arguments
}

fn default_tool_log_path(tool_key: &str, working_directory: Option<&PathBuf>) -> Option<PathBuf> {
    let file_name = format!("logfile-{}.log", tool_key.to_ascii_lowercase());
    working_directory.map(|directory| directory.join(file_name))
}

pub fn build_command_preview(request: &ToolLaunchRequest) -> SlimResult<CommandPreview> {
    match request.runner_type {
        RunnerType::Native => Ok(CommandPreview {
            program: request.executable_path.to_string_lossy().to_string(),
            args: request.arguments.clone(),
            working_directory: request.working_directory.clone(),
            env: Vec::new(),
            process_id: None,
            execution: None,
        }),
        RunnerType::Wine => {
            let mut env = Vec::new();
            if let Some(prefix) = &request.wine_prefix {
                env.push(("WINEPREFIX".into(), prefix.to_string_lossy().to_string()));
            }
            let mut args = vec![request.executable_path.to_string_lossy().to_string()];
            args.extend(request.arguments.clone());
            Ok(CommandPreview {
                program: "wine".into(),
                args,
                working_directory: request.working_directory.clone(),
                env,
                process_id: None,
                execution: None,
            })
        }
        RunnerType::Proton => {
            let mut env = Vec::new();
            if let Some(prefix) = &request.wine_prefix {
                env.push((
                    "STEAM_COMPAT_DATA_PATH".into(),
                    prefix.to_string_lossy().to_string(),
                ));
            }
            if let Some(working_directory) = &request.working_directory {
                env.push((
                    "STEAM_COMPAT_INSTALL_PATH".into(),
                    working_directory.to_string_lossy().to_string(),
                ));
            }
            if let Ok(client_path) = std::env::var("STEAM_COMPAT_CLIENT_INSTALL_PATH") {
                env.push(("STEAM_COMPAT_CLIENT_INSTALL_PATH".into(), client_path));
            }

            let mut args = vec![
                "run".into(),
                request.executable_path.to_string_lossy().to_string(),
            ];
            args.extend(request.arguments.clone());
            Ok(CommandPreview {
                program: std::env::var("SLIMCC_PROTON_BIN").unwrap_or_else(|_| "proton".into()),
                args,
                working_directory: request.working_directory.clone(),
                env,
                process_id: None,
                execution: None,
            })
        }
        RunnerType::Lutris => {
            let mut args = vec![
                "--exec".into(),
                request.executable_path.to_string_lossy().to_string(),
            ];
            args.extend(request.arguments.clone());
            Ok(CommandPreview {
                program: std::env::var("SLIMCC_LUTRIS_BIN").unwrap_or_else(|_| "lutris".into()),
                args,
                working_directory: request.working_directory.clone(),
                env: Vec::new(),
                process_id: None,
                execution: None,
            })
        }
    }
}

fn row_to_tool_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolProfile> {
    let arguments_json: String = row.get(5)?;
    let arguments: Vec<String> = serde_json::from_str(&arguments_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(ToolProfile {
        id: row.get(0)?,
        tool_key: row.get(1)?,
        display_name: row.get(2)?,
        executable_path: optional_path(row.get::<_, Option<String>>(3)?),
        runner_type: row.get(4)?,
        arguments,
        working_directory: optional_path(row.get::<_, Option<String>>(6)?),
        wine_prefix: optional_path(row.get::<_, Option<String>>(7)?),
        log_path: optional_path(row.get::<_, Option<String>>(8)?),
        enabled: row.get::<_, i64>(9)? != 0,
    })
}

fn row_to_tool_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolRunRecord> {
    let arguments_json: String = row.get(4)?;
    let arguments: Vec<String> = serde_json::from_str(&arguments_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(ToolRunRecord {
        id: row.get(0)?,
        tool_key: row.get(1)?,
        display_name: row.get(2)?,
        program: row.get(3)?,
        arguments,
        working_directory: optional_path(row.get::<_, Option<String>>(5)?),
        exit_code: row.get(6)?,
        stdout: row.get(7)?,
        stderr: row.get(8)?,
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
    })
}

fn optional_path(value: Option<String>) -> Option<PathBuf> {
    value
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

fn path_to_string(path: Option<&PathBuf>) -> Option<String> {
    path.map(|path| path.to_string_lossy().to_string())
        .filter(|path| !path.trim().is_empty())
}

fn normalize_runner_type(value: &str) -> SlimResult<String> {
    Ok(match value.to_ascii_lowercase().as_str() {
        "native" => "Native",
        "wine" => "Wine",
        "proton" => "Proton",
        "lutris" => "Lutris",
        other => {
            return Err(SlimError::InvalidPath(format!(
                "unsupported runner type: {other}"
            )));
        }
    }
    .to_string())
}

fn runner_type_from_label(value: &str) -> SlimResult<RunnerType> {
    Ok(match normalize_runner_type(value)?.as_str() {
        "Native" => RunnerType::Native,
        "Wine" => RunnerType::Wine,
        "Proton" => RunnerType::Proton,
        "Lutris" => RunnerType::Lutris,
        _ => unreachable!("normalize_runner_type only returns supported runners"),
    })
}

pub fn spawn_tool(request: &ToolLaunchRequest) -> SlimResult<Child> {
    let preview = build_command_preview(request)?;
    let mut command = Command::new(&preview.program);
    command.args(&preview.args);
    if let Some(cwd) = &preview.working_directory {
        command.current_dir(cwd);
    }
    for (key, value) in &preview.env {
        command.env(key, value);
    }
    Ok(command.spawn()?)
}

pub fn collect_tool_output(child: Child) -> SlimResult<ToolExecutionResult> {
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(ToolExecutionResult {
        exit_code: output.status.code(),
        stdout: filter_tool_output(&stdout),
        stderr: filter_tool_output(&stderr),
    })
}

fn filter_tool_output(raw: &str) -> Option<String> {
    let filtered: String = raw
        .lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            !lower.contains(":fixme:")
                && !lower.starts_with("fixme:")
                && !lower.contains("winediag:loader_init")
                && !lower.contains("stub!")
        })
        .map(|line| {
            let mut out = line.to_string();
            out.push('\n');
            out
        })
        .collect();

    let trimmed = filtered.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(filtered)
    }
}

pub fn launch_tool(request: &ToolLaunchRequest) -> SlimResult<(u32, ToolExecutionResult)> {
    let child = spawn_tool(request)?;
    let pid = child.id();
    let execution = collect_tool_output(child)?;
    Ok((pid, execution))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::path::PathBuf;

    #[test]
    fn upsert_tool_profile_persists_runner_arguments_and_paths() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        initialize_tool_profile_schema(&conn).expect("create tool profile schema");

        let saved = upsert_tool_profile(
            &mut conn,
            UpdateToolProfileRequest {
                tool_key: "xedit".into(),
                display_name: "SSEEdit".into(),
                executable_path: Some(PathBuf::from("/tools/SSEEdit.exe")),
                runner_type: "Wine".into(),
                arguments: vec!["-SSE".into()],
                working_directory: Some(PathBuf::from("/tools")),
                wine_prefix: Some(PathBuf::from("/prefixes/skyrim")),
                log_path: Some(PathBuf::from("/logs/xedit.log")),
                enabled: true,
            },
        )
        .expect("save tool profile");

        assert_eq!(saved.tool_key, "xedit");
        assert_eq!(saved.arguments, vec!["-SSE"]);

        let profiles = list_tool_profiles(&conn).expect("list tool profiles");
        let xedit = profiles
            .into_iter()
            .find(|profile| profile.tool_key == "xedit")
            .expect("xEdit profile exists");

        assert_eq!(xedit.display_name, "SSEEdit");
        assert_eq!(
            xedit.executable_path,
            Some(PathBuf::from("/tools/SSEEdit.exe"))
        );
        assert_eq!(xedit.runner_type, "Wine");
        assert_eq!(xedit.working_directory, Some(PathBuf::from("/tools")));
        assert_eq!(xedit.wine_prefix, Some(PathBuf::from("/prefixes/skyrim")));
        assert_eq!(xedit.log_path, Some(PathBuf::from("/logs/xedit.log")));
        assert!(xedit.enabled);
    }

    #[test]
    fn launch_tool_profile_uses_settings_for_empty_working_directory_and_wine_prefix() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .expect("create settings schema");
        initialize_tool_profile_schema(&conn).expect("create tool profile schema");
        conn.execute(
            "INSERT INTO app_settings (key, value)
             VALUES ('default_install_path', '/games/Skyrim Special Edition'),
                    ('default_data_path', '/games/Skyrim Special Edition/Data'),
                    ('default_wine_prefix', '/wine/skyrim')",
            [],
        )
        .expect("insert settings");

        upsert_tool_profile(
            &mut conn,
            UpdateToolProfileRequest {
                tool_key: "xedit".into(),
                display_name: "SSEEdit".into(),
                executable_path: Some(PathBuf::from("/tools/SSEEdit.exe")),
                runner_type: "Wine".into(),
                arguments: vec!["-SSE".into()],
                working_directory: None,
                wine_prefix: None,
                log_path: None,
                enabled: true,
            },
        )
        .expect("save tool profile");

        let profile = get_tool_profile(&conn, "xedit").expect("load profile");
        let settings = crate::settings::load_settings(&conn).expect("load settings");
        let executable_path = profile.executable_path.clone().unwrap();
        let launch_request =
            build_tool_launch_request(&profile, executable_path, &settings).expect("build request");
        let preview = build_command_preview(&launch_request).expect("build preview");

        assert_eq!(
            preview.working_directory,
            Some(PathBuf::from("/games/Skyrim Special Edition"))
        );
        assert!(preview
            .env
            .contains(&("WINEPREFIX".into(), "/wine/skyrim".into())));
    }

    #[test]
    fn xedit_launch_defaults_to_sse_argument_when_missing() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .expect("create settings schema");
        initialize_tool_profile_schema(&conn).expect("create tool profile schema");

        upsert_tool_profile(
            &mut conn,
            UpdateToolProfileRequest {
                tool_key: "xedit".into(),
                display_name: "SSEEdit".into(),
                executable_path: Some(PathBuf::from("/tools/SSEEdit.exe")),
                runner_type: "Wine".into(),
                arguments: Vec::new(),
                working_directory: Some(PathBuf::from("/tools")),
                wine_prefix: None,
                log_path: None,
                enabled: true,
            },
        )
        .expect("save tool profile");

        let profile = get_tool_profile(&conn, "xedit").expect("load profile");
        let settings = crate::settings::load_settings(&conn).expect("load settings");
        let request = build_tool_launch_request(
            &profile,
            profile.executable_path.clone().unwrap(),
            &settings,
        )
        .expect("build request");

        assert_eq!(request.arguments.first().map(String::as_str), Some("-SSE"));
    }

    #[test]
    fn xedit_launch_keeps_existing_sse_argument_without_duplicates() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .expect("create settings schema");
        initialize_tool_profile_schema(&conn).expect("create tool profile schema");

        upsert_tool_profile(
            &mut conn,
            UpdateToolProfileRequest {
                tool_key: "xedit".into(),
                display_name: "SSEEdit".into(),
                executable_path: Some(PathBuf::from("/tools/SSEEdit.exe")),
                runner_type: "Wine".into(),
                arguments: vec!["-SSE".into(), "-someflag".into()],
                working_directory: Some(PathBuf::from("/tools")),
                wine_prefix: None,
                log_path: None,
                enabled: true,
            },
        )
        .expect("save tool profile");

        let profile = get_tool_profile(&conn, "xedit").expect("load profile");
        let settings = crate::settings::load_settings(&conn).expect("load settings");
        let request = build_tool_launch_request(
            &profile,
            profile.executable_path.clone().unwrap(),
            &settings,
        )
        .expect("build request");

        assert_eq!(
            request.arguments,
            vec!["-SSE".to_string(), "-someflag".to_string()]
        );
    }

    #[test]
    fn proton_preview_runs_executable_through_proton_run() {
        let request = ToolLaunchRequest {
            runner_type: RunnerType::Proton,
            executable_path: PathBuf::from("/tools/SSEEdit/SSEEdit.exe"),
            arguments: vec!["-SSE".into()],
            working_directory: Some(PathBuf::from("/games/Skyrim Special Edition")),
            wine_prefix: Some(PathBuf::from("/compat/skyrim")),
        };

        let preview = build_command_preview(&request).expect("build proton preview");

        assert_eq!(preview.program, "proton");
        assert_eq!(
            preview.args,
            vec!["run", "/tools/SSEEdit/SSEEdit.exe", "-SSE"]
        );
        assert!(preview
            .env
            .iter()
            .any(|(key, value)| { key == "STEAM_COMPAT_DATA_PATH" && value == "/compat/skyrim" }));
        assert!(preview.env.iter().any(|(key, value)| {
            key == "STEAM_COMPAT_INSTALL_PATH" && value == "/games/Skyrim Special Edition"
        }));
    }

    #[test]
    fn lutris_preview_executes_program_with_lutris_runtime() {
        let request = ToolLaunchRequest {
            runner_type: RunnerType::Lutris,
            executable_path: PathBuf::from("/tools/Nemesis/Nemesis.exe"),
            arguments: vec!["--check".into()],
            working_directory: Some(PathBuf::from(
                "/games/Skyrim Special Edition/Data/Nemesis_Engine",
            )),
            wine_prefix: None,
        };

        let preview = build_command_preview(&request).expect("build lutris preview");

        assert_eq!(preview.program, "lutris");
        assert_eq!(
            preview.args,
            vec!["--exec", "/tools/Nemesis/Nemesis.exe", "--check"]
        );
        assert_eq!(
            preview.working_directory,
            Some(PathBuf::from(
                "/games/Skyrim Special Edition/Data/Nemesis_Engine"
            ))
        );
    }

    #[test]
    fn tool_validation_uses_derived_log_path_when_profile_has_none() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .expect("create settings schema");
        initialize_tool_profile_schema(&conn).expect("create tool profile schema");
        conn.execute(
            "INSERT INTO app_settings (key, value)
             VALUES ('default_install_path', '/games/Skyrim Special Edition'),
                    ('default_data_path', '/games/Skyrim Special Edition/Data'),
                    ('default_wine_prefix', '/wine/skyrim')",
            [],
        )
        .expect("insert settings");

        upsert_tool_profile(
            &mut conn,
            UpdateToolProfileRequest {
                tool_key: "loot".into(),
                display_name: "LOOT".into(),
                executable_path: Some(PathBuf::from("/tools/loot")),
                runner_type: "Native".into(),
                arguments: Vec::new(),
                working_directory: Some(PathBuf::from("/tools/loot")),
                wine_prefix: None,
                log_path: None,
                enabled: true,
            },
        )
        .expect("save tool profile");

        let validations = validate_tool_profiles(&conn).expect("validate tools");
        let loot = validations
            .into_iter()
            .find(|entry| entry.tool_key == "loot")
            .expect("loot validation");

        assert_eq!(
            loot.effective_log_path,
            Some(PathBuf::from("/tools/loot/logfile-loot.log"))
        );
        assert!(loot
            .notes
            .iter()
            .any(|note| note.contains("Log path defaults to")));
    }

    #[test]
    fn nemesis_without_working_directory_uses_executable_parent() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .expect("create settings schema");
        initialize_tool_profile_schema(&conn).expect("create tool profile schema");
        conn.execute(
            "INSERT INTO app_settings (key, value)
             VALUES ('default_install_path', '/games/Skyrim Special Edition'),
                    ('default_data_path', '/games/Skyrim Special Edition/Data'),
                    ('default_wine_prefix', '/wine/skyrim')",
            [],
        )
        .expect("insert settings");

        upsert_tool_profile(
            &mut conn,
            UpdateToolProfileRequest {
                tool_key: "nemesis".into(),
                display_name: "Nemesis".into(),
                executable_path: Some(PathBuf::from(
                    "/games/Skyrim Special Edition/Data/Nemesis_Engine/Nemesis Unlimited Behavior Engine.exe",
                )),
                runner_type: "Wine".into(),
                arguments: Vec::new(),
                working_directory: None,
                wine_prefix: None,
                log_path: None,
                enabled: true,
            },
        )
        .expect("save tool profile");

        let profile = get_tool_profile(&conn, "nemesis").expect("load profile");
        let settings = crate::settings::load_settings(&conn).expect("load settings");
        let executable_path = profile.executable_path.clone().unwrap();
        let launch_request =
            build_tool_launch_request(&profile, executable_path, &settings).expect("build request");

        assert_eq!(
            launch_request.working_directory,
            Some(PathBuf::from(
                "/games/Skyrim Special Edition/Data/Nemesis_Engine"
            ))
        );
    }

    #[test]
    fn record_tool_run_persists_captured_output() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/006_tool_runs.sql"))
            .expect("create tool runs schema");
        let preview = CommandPreview {
            program: "echo".into(),
            args: vec!["hello".into()],
            working_directory: Some(PathBuf::from("/tmp")),
            env: Vec::new(),
            process_id: None,
            execution: None,
        };
        let execution = ToolExecutionResult {
            exit_code: Some(0),
            stdout: Some("hello\n".into()),
            stderr: None,
        };

        record_tool_run(
            &conn,
            "test",
            "Test Tool",
            &preview,
            &execution,
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:01Z",
        )
        .expect("record run");

        let runs = list_recent_tool_runs(&conn, 10).expect("list runs");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].tool_key, "test");
        assert_eq!(runs[0].stdout.as_deref(), Some("hello\n"));
        assert_eq!(runs[0].arguments, vec!["hello"]);
    }

    #[test]
    fn filter_tool_output_removes_wine_fixmes_but_keeps_errors() {
        let raw = "\
002c:fixme:winediag:loader_init wine-staging 11.8 is a testing version containing experimental patches.\n\
00c4:err:hid:udev_bus_init UDEV monitor creation failed\n\
0128:fixme:shell:SHGetStockIconInfo flags 0x101 not implemented\n";

        let filtered = filter_tool_output(raw);

        assert_eq!(
            filtered.as_deref(),
            Some("00c4:err:hid:udev_bus_init UDEV monitor creation failed\n")
        );
    }
}
