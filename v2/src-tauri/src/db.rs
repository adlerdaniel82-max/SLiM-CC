use crate::error::SlimResult;
use rusqlite::Connection;
use std::path::Path;

const INITIAL_SCHEMA: &str = include_str!("../migrations/001_initial.sql");
const MOD_EDITOR_STATE_SCHEMA: &str = include_str!("../migrations/002_mod_editor_state.sql");
const MOD_DEPENDENCIES_SCHEMA: &str = include_str!("../migrations/004_mod_dependencies.sql");
const NEXUS_METADATA_SCHEMA: &str = include_str!("../migrations/005_nexus_metadata.sql");
const TOOL_RUNS_SCHEMA: &str = include_str!("../migrations/006_tool_runs.sql");
const DIAGNOSTIC_FINDINGS_SCHEMA: &str = include_str!("../migrations/007_diagnostic_findings.sql");
const NEXUS_API_CACHE_SCHEMA: &str = include_str!("../migrations/008_nexus_api_cache.sql");
const NEXUS_V3_FILE_MAPPING_SCHEMA: &str =
    include_str!("../migrations/009_nexus_v3_file_mapping.sql");
const INSTANCE_GAME_STARTER_SCHEMA: &str =
    include_str!("../migrations/010_instance_game_starter.sql");
const PROFILE_PLUGINS_SCHEMA: &str = include_str!("../migrations/011_profile_plugins.sql");
const DEPENDENCY_STATE_SCHEMA: &str = include_str!("../migrations/012_dependency_state.sql");

pub fn open_database(path: &Path) -> SlimResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    conn.execute_batch(INITIAL_SCHEMA)?;
    conn.execute_batch(MOD_EDITOR_STATE_SCHEMA)?;
    crate::tools::initialize_tool_profile_schema(&conn)?;
    conn.execute_batch(MOD_DEPENDENCIES_SCHEMA)?;
    conn.execute_batch(NEXUS_METADATA_SCHEMA)?;
    conn.execute_batch(TOOL_RUNS_SCHEMA)?;
    conn.execute_batch(DIAGNOSTIC_FINDINGS_SCHEMA)?;
    conn.execute_batch(NEXUS_API_CACHE_SCHEMA)?;
    let _ = conn.execute_batch(NEXUS_V3_FILE_MAPPING_SCHEMA);
    let _ = conn.execute_batch(INSTANCE_GAME_STARTER_SCHEMA);
    conn.execute_batch(PROFILE_PLUGINS_SCHEMA)?;
    let _ = conn.execute_batch(DEPENDENCY_STATE_SCHEMA);
    Ok(conn)
}
