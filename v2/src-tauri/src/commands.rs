use crate::app_state::AppState;
use crate::conflicts;
use crate::deploy;
use crate::error::SlimResult;
use crate::instance;
use crate::models::{
    AnalyzeNexusCollectionRequest, CreateInstanceRequest, CreateProfileRequest, DeployAction,
    DeployPlan, DeployTarget, DiagnosisReport, FomodPackagePreview, GameInstance,
    ImportModFolderRequest, ImportedModReport, LaunchLootRequest, LootLaunchResult,
    LootSortPreview, ModConflictSummary, ModDependencyStatus, ModDependencySummary,
    ModDownloadCandidate, ModFile, NexusApiStatus, NexusCollectionAnalysis, NexusModCache,
    NexusModLink, NexusRequirement, NexusRequirementStatus, NexusSourceLink, NexusSyncResult,
    Profile, ProfileComparison, ProfileModEntry, ProfilePluginEntry, ScanModFilesRequest,
    StoredDiagnosticFinding, ToolProfileValidation, ToolRunRecord, UpdateInstanceRequest,
    UpdateModDependenciesRequest, UpdateModRequest, UpdateNexusModLinkRequest,
    UpdateProfileModsRequest, UpdateProfilePluginsRequest, UpdateProfileRequest, WorkspaceInfo,
};
use crate::mods;
use crate::settings::{self, AppSettings, UpdateAppSettingsRequest, UpdateNexusApiKeyRequest};
use crate::staging;
use crate::tools;
use crate::workspace;
use std::path::Path;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn workspace_info(state: State<AppState>) -> SlimResult<WorkspaceInfo> {
    Ok(state.workspace_info())
}

#[tauri::command]
pub fn create_instance(
    state: State<AppState>,
    request: CreateInstanceRequest,
) -> SlimResult<GameInstance> {
    let workspace_root = state.workspace_root.clone();
    state.with_connection(|conn| instance::create_instance(conn, &workspace_root, request))
}

#[tauri::command]
pub fn update_instance(
    state: State<AppState>,
    request: UpdateInstanceRequest,
) -> SlimResult<GameInstance> {
    state.with_connection(|conn| instance::update_instance(conn, request))
}

#[tauri::command]
pub fn delete_instance(state: State<AppState>, instance_id: String) -> SlimResult<()> {
    let workspace_root = state.workspace_root.clone();
    state.with_connection_mut(|conn| instance::delete_instance(conn, &workspace_root, &instance_id))
}

#[tauri::command]
pub fn list_instances(state: State<AppState>) -> SlimResult<Vec<GameInstance>> {
    state.with_connection(instance::list_instances)
}

#[tauri::command]
pub fn list_profiles(state: State<AppState>, instance_id: String) -> SlimResult<Vec<Profile>> {
    state.with_connection(|conn| workspace::list_profiles(conn, &instance_id))
}

#[tauri::command]
pub fn list_mods(
    state: State<AppState>,
    instance_id: String,
) -> SlimResult<Vec<crate::models::ModRecord>> {
    state.with_connection(|conn| workspace::list_mods(conn, &instance_id))
}

#[tauri::command]
pub fn update_mod(
    state: State<AppState>,
    request: UpdateModRequest,
) -> SlimResult<crate::models::ModRecord> {
    state.with_connection_mut(|conn| mods::update_mod(conn, request))
}

#[tauri::command]
pub fn delete_mod(state: State<AppState>, mod_id: String) -> SlimResult<()> {
    let workspace_root = state.workspace_root.clone();
    state.with_connection_mut(|conn| mods::delete_mod(conn, &workspace_root, &mod_id))
}

#[tauri::command]
pub fn list_mod_dependencies(
    state: State<AppState>,
    mod_id: String,
) -> SlimResult<Vec<ModDependencyStatus>> {
    state.with_connection(|conn| mods::list_mod_dependencies(conn, &mod_id))
}

#[tauri::command]
pub fn list_instance_dependency_summary(
    state: State<AppState>,
    instance_id: String,
    profile_id: Option<String>,
) -> SlimResult<Vec<ModDependencySummary>> {
    state.with_connection(|conn| {
        mods::list_instance_dependency_summary_for_profile(
            conn,
            &instance_id,
            profile_id.as_deref(),
        )
    })
}

#[tauri::command]
pub fn set_manual_dependency_satisfied(
    state: State<AppState>,
    dependency_id: String,
    satisfied: bool,
) -> SlimResult<()> {
    state.with_connection(|conn| {
        mods::set_manual_dependency_satisfied(conn, &dependency_id, satisfied)
    })
}

#[tauri::command]
pub fn update_mod_dependencies(
    state: State<AppState>,
    request: UpdateModDependenciesRequest,
) -> SlimResult<Vec<ModDependencyStatus>> {
    state.with_connection_mut(|conn| mods::update_mod_dependencies(conn, request))
}

#[tauri::command]
pub fn list_nexus_mod_links(
    state: State<AppState>,
    instance_id: String,
) -> SlimResult<Vec<NexusModLink>> {
    state.with_connection(|conn| crate::nexus::list_nexus_mod_links(conn, &instance_id))
}

#[tauri::command]
pub fn upsert_nexus_mod_link(
    state: State<AppState>,
    request: UpdateNexusModLinkRequest,
) -> SlimResult<Option<NexusModLink>> {
    state.with_connection(|conn| crate::nexus::upsert_nexus_mod_link(conn, request))
}

#[tauri::command]
pub fn parse_nexus_source_link(source_url: String) -> Option<NexusSourceLink> {
    crate::nexus::parse_nexus_source_link(&source_url)
}

#[tauri::command]
pub fn analyze_nexus_collection(
    state: State<AppState>,
    request: AnalyzeNexusCollectionRequest,
) -> SlimResult<NexusCollectionAnalysis> {
    state.with_connection(|conn| crate::nexus::analyze_nexus_collection(conn, request))
}

#[tauri::command]
pub fn download_nexus_file(
    state: State<AppState>,
    request: crate::models::NexusDownloadRequest,
) -> SlimResult<crate::models::NexusDownloadResult> {
    state.with_connection(|conn| crate::nexus::download_nexus_file(conn, request))
}

#[tauri::command]
pub fn list_nexus_requirements(
    state: State<AppState>,
    mod_id: String,
) -> SlimResult<Vec<NexusRequirement>> {
    state.with_connection(|conn| crate::nexus::list_nexus_requirements(conn, &mod_id))
}

#[tauri::command]
pub fn list_nexus_requirement_status(
    state: State<AppState>,
    mod_id: String,
) -> SlimResult<Vec<NexusRequirementStatus>> {
    state.with_connection(|conn| crate::nexus::list_nexus_requirement_status(conn, &mod_id))
}

#[tauri::command]
pub fn list_nexus_mod_cache(
    state: State<AppState>,
    instance_id: String,
) -> SlimResult<Vec<NexusModCache>> {
    state.with_connection(|conn| crate::nexus::list_nexus_mod_cache(conn, &instance_id))
}

#[tauri::command]
pub fn validate_nexus_api(state: State<AppState>) -> SlimResult<NexusApiStatus> {
    state.with_connection(crate::nexus::validate_nexus_api)
}

#[tauri::command]
pub fn sync_nexus_mod(state: State<AppState>, mod_id: String) -> SlimResult<NexusSyncResult> {
    state.with_connection(|conn| crate::nexus::sync_nexus_mod(conn, &mod_id))
}

#[tauri::command]
pub fn create_profile(
    state: State<AppState>,
    request: CreateProfileRequest,
) -> SlimResult<Profile> {
    state.with_connection_mut(|conn| crate::profiles::create_profile(conn, request))
}

#[tauri::command]
pub fn update_profile(
    state: State<AppState>,
    request: UpdateProfileRequest,
) -> SlimResult<Profile> {
    state.with_connection_mut(|conn| crate::profiles::update_profile(conn, request))
}

#[tauri::command]
pub fn delete_profile(state: State<AppState>, profile_id: String) -> SlimResult<()> {
    let workspace_root = state.workspace_root.clone();
    state.with_connection_mut(|conn| {
        crate::profiles::delete_profile(conn, &workspace_root, &profile_id)
    })
}

#[tauri::command]
pub fn list_profile_mods(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<Vec<ProfileModEntry>> {
    state.with_connection(|conn| mods::list_profile_mods(conn, &instance_id, &profile_id))
}

#[tauri::command]
pub fn update_profile_mods(
    state: State<AppState>,
    request: UpdateProfileModsRequest,
) -> SlimResult<()> {
    state.with_connection_mut(|conn| mods::update_profile_mods(conn, request))
}

#[tauri::command]
pub fn list_profile_plugins(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<Vec<ProfilePluginEntry>> {
    state.with_connection(|conn| mods::list_profile_plugins(conn, &instance_id, &profile_id))
}

#[tauri::command]
pub fn update_profile_plugins(
    state: State<AppState>,
    request: UpdateProfilePluginsRequest,
) -> SlimResult<Vec<ProfilePluginEntry>> {
    state.with_connection_mut(|conn| mods::update_profile_plugins(conn, request))
}

#[tauri::command]
pub async fn import_mod_folder(
    state: State<'_, AppState>,
    request: ImportModFolderRequest,
) -> SlimResult<ImportedModReport> {
    let workspace_root = state.workspace_root.clone();
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = crate::db::open_database(&database_path)?;
        mods::import_mod_folder(&mut conn, &workspace_root, request)
    })
    .await
    .map_err(|error| crate::error::SlimError::Process(format!("import worker failed: {error}")))?
}

#[tauri::command]
pub async fn preview_fomod_package(
    state: State<'_, AppState>,
    package_root: String,
    instance_id: Option<String>,
    profile_id: Option<String>,
) -> SlimResult<FomodPackagePreview> {
    let package_root = PathBuf::from(package_root);
    let dependency_context = match instance_id.as_deref() {
        Some(instance_id) => Some(state.with_connection(|conn| {
            mods::build_fomod_dependency_context_for_profile(
                conn,
                instance_id,
                profile_id.as_deref(),
            )
        })?),
        None => None,
    };
    let workspace_root = state.workspace_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        mods::preview_fomod_package_with_context(
            &workspace_root,
            package_root.as_path(),
            None,
            dependency_context.as_ref(),
        )
    })
    .await
    .map_err(|error| crate::error::SlimError::Process(format!("preview worker failed: {error}")))?
}

#[tauri::command]
pub fn discard_fomod_preview(state: State<AppState>, preview_path: String) -> SlimResult<()> {
    mods::discard_fomod_preview(&state.workspace_root, Path::new(&preview_path))
}

#[tauri::command]
pub fn preview_mod_fomod(
    state: State<AppState>,
    mod_id: String,
) -> SlimResult<FomodPackagePreview> {
    state.with_connection(|conn| {
        let _mod_record = workspace::get_mod_by_id(conn, &mod_id)?;
        mods::preview_mod_fomod(conn, &mod_id)
    })
}

#[tauri::command]
pub fn launch_tool(request: tools::ToolLaunchRequest) -> SlimResult<tools::CommandPreview> {
    let mut preview = tools::build_command_preview(&request)?;
    let (process_id, execution) = tools::launch_tool(&request)?;
    preview.process_id = Some(process_id);
    preview.execution = Some(execution);
    Ok(preview)
}

#[tauri::command]
pub fn process_is_running(process_id: u32) -> bool {
    let stat = match std::fs::read_to_string(format!("/proc/{process_id}/stat")) {
        Ok(stat) => stat,
        Err(_) => return false,
    };
    stat.rsplit_once(") ")
        .and_then(|(_, fields)| fields.chars().next())
        .map(|state| state != 'Z' && state != 'X')
        .unwrap_or(false)
}

#[tauri::command]
pub fn launch_game(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<tools::CommandPreview> {
    let vfs_status = prepare_profile_vfs(state.clone(), instance_id.clone(), profile_id.clone())?;
    state.with_connection(|conn| {
        let instance = instance::get_instance_by_id(conn, &instance_id)?;
        let settings = settings::load_settings(conn)?;
        let real_executable_path = instance.game_starter_path.clone().ok_or_else(|| {
            crate::error::SlimError::InvalidPath("Game starter is not configured".into())
        })?;
        if !real_executable_path.is_file() {
            return Err(crate::error::SlimError::InvalidPath(format!(
                "Game starter not found: {}",
                real_executable_path.display()
            )));
        }
        let runner_type = match instance.runner_type.to_ascii_lowercase().as_str() {
            "manual" | "native" => tools::RunnerType::Native,
            "wine" => tools::RunnerType::Wine,
            "proton" => tools::RunnerType::Proton,
            "lutris" => tools::RunnerType::Lutris,
            other => {
                return Err(crate::error::SlimError::InvalidPath(format!(
                    "unsupported game runner type: {other}"
                )));
            }
        };
        let wine_prefix = instance.wine_prefix.or(settings.wine_prefix);
        if matches!(
            runner_type,
            tools::RunnerType::Wine | tools::RunnerType::Proton
        ) {
            instance::validate_wine_prefix(&instance.install_path, wine_prefix.as_deref())?;
        }
        let executable_relative = real_executable_path
            .strip_prefix(&instance.install_path)
            .map_err(|_| {
                crate::error::SlimError::Safety(
                    "game starter must be inside the configured game directory".into(),
                )
            })?;
        let executable_path = vfs_status.mount_path.join(executable_relative);
        let request = tools::ToolLaunchRequest {
            runner_type,
            executable_path,
            arguments: Vec::new(),
            working_directory: Some(vfs_status.mount_path),
            wine_prefix,
        };
        let mut preview = tools::build_command_preview(&request)?;
        let child = tools::spawn_tool(&request)?;
        preview.process_id = Some(child.id());
        Ok(preview)
    })
}

#[tauri::command]
pub fn prepare_profile_vfs(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<crate::models::VfsStatus> {
    let instance =
        state.with_connection(|conn| instance::get_instance_by_id(conn, &instance_id))?;
    {
        let mut vfs = state
            .vfs
            .lock()
            .map_err(|_| crate::error::SlimError::Safety("VFS lock poisoned".into()))?;
        if vfs
            .status(&state.workspace_root, &instance_id, &profile_id)
            .mounted
        {
            vfs.unmount(&state.workspace_root, &instance_id, &profile_id)?;
        }
    }
    let plan = state.with_connection(|conn| {
        deploy::build_plan(
            conn,
            &state.workspace_root,
            &instance_id,
            &profile_id,
            DeployTarget::Staging,
            DeployAction::Symlink,
        )
    })?;
    staging::execute_staging_plan(&state.workspace_root, &plan)?;
    let mut vfs = state
        .vfs
        .lock()
        .map_err(|_| crate::error::SlimError::Safety("VFS lock poisoned".into()))?;
    vfs.mount(
        &state.workspace_root,
        &instance_id,
        &profile_id,
        &instance.install_path,
    )
}

#[tauri::command]
pub fn unmount_profile_vfs(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<crate::models::VfsStatus> {
    let mut vfs = state
        .vfs
        .lock()
        .map_err(|_| crate::error::SlimError::Safety("VFS lock poisoned".into()))?;
    vfs.unmount(&state.workspace_root, &instance_id, &profile_id)
}

#[tauri::command]
pub fn profile_vfs_status(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<crate::models::VfsStatus> {
    let vfs = state
        .vfs
        .lock()
        .map_err(|_| crate::error::SlimError::Safety("VFS lock poisoned".into()))?;
    Ok(vfs.status(&state.workspace_root, &instance_id, &profile_id))
}

#[tauri::command]
pub fn list_tool_profiles(state: State<AppState>) -> SlimResult<Vec<tools::ToolProfile>> {
    state.with_connection(tools::list_tool_profiles)
}

#[tauri::command]
pub fn list_tool_executable_candidates(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<Vec<tools::ToolExecutableCandidate>> {
    state.with_connection(|conn| {
        tools::list_tool_executable_candidates(conn, &instance_id, &profile_id)
    })
}

#[tauri::command]
pub fn upsert_tool_profile(
    state: State<AppState>,
    request: tools::UpdateToolProfileRequest,
) -> SlimResult<tools::ToolProfile> {
    state.with_connection_mut(|conn| tools::upsert_tool_profile(conn, request))
}

#[tauri::command]
pub fn launch_tool_profile(
    state: State<AppState>,
    request: tools::LaunchToolProfileRequest,
) -> SlimResult<tools::CommandPreview> {
    state.with_connection_mut(|conn| tools::launch_tool_profile(conn, request))
}

#[tauri::command]
pub fn launch_tool_profile_vfs(
    state: State<AppState>,
    tool_key: String,
    instance_id: String,
    profile_id: String,
) -> SlimResult<tools::CommandPreview> {
    let vfs_status = prepare_profile_vfs(state.clone(), instance_id.clone(), profile_id.clone())?;
    state.with_connection(|conn| {
        let instance = instance::get_instance_by_id(conn, &instance_id)?;
        let profile = tools::get_tool_profile(conn, &tool_key)?;
        if !profile.enabled {
            return Err(crate::error::SlimError::Disabled(format!(
                "Werkzeug ist deaktiviert: {}",
                profile.display_name
            )));
        }
        let configured_executable = profile.executable_path.clone().ok_or_else(|| {
            crate::error::SlimError::InvalidPath(format!(
                "Programmdatei ist nicht konfiguriert: {}",
                profile.display_name
            ))
        })?;
        let executable_path = resolve_tool_path_into_vfs(
            conn,
            &instance_id,
            &profile_id,
            &configured_executable,
            &instance.install_path,
            &vfs_status.mount_path,
        )?;
        if !executable_path.is_file() {
            return Err(crate::error::SlimError::NotFound(format!(
                "Programmdatei ist im VFS nicht vorhanden: {}",
                executable_path.display()
            )));
        }

        let mut settings = settings::load_settings(conn)?;
        settings.install_path = Some(instance.install_path.clone());
        if settings.wine_prefix.is_none() {
            settings.wine_prefix = instance.wine_prefix.clone();
        }
        let mut request =
            tools::build_tool_launch_request(&profile, configured_executable, &settings)?;
        request.executable_path = executable_path;
        request.working_directory = match profile.working_directory.as_deref() {
            Some(path) => Some(resolve_tool_path_into_vfs(
                conn,
                &instance_id,
                &profile_id,
                path,
                &instance.install_path,
                &vfs_status.mount_path,
            )?),
            None if matches!(
                profile.tool_key.to_ascii_lowercase().as_str(),
                "nemesis" | "fnis" | "pandora" | "bodyslide"
            ) =>
            {
                request.executable_path.parent().map(PathBuf::from)
            }
            None => Some(vfs_status.mount_path.clone()),
        };

        let mut preview = tools::build_command_preview(&request)?;
        let child = tools::spawn_tool(&request)?;
        preview.process_id = Some(child.id());
        Ok(preview)
    })
}

fn resolve_tool_path_into_vfs(
    conn: &rusqlite::Connection,
    instance_id: &str,
    profile_id: &str,
    path: &Path,
    game_root: &Path,
    mount_path: &Path,
) -> SlimResult<PathBuf> {
    let relative = if path.is_relative() {
        if path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            return Err(crate::error::SlimError::Safety(format!(
                "Ungültiger relativer VFS-Werkzeugpfad: {}",
                path.display()
            )));
        }
        PathBuf::from(crate::scanner::normalize_rel_path(&path.to_string_lossy()))
    } else if let Ok(relative) = path.strip_prefix(game_root) {
        relative.to_path_buf()
    } else {
        let normalized_rel_path: String = conn
            .query_row(
                "SELECT mf.normalized_rel_path
                 FROM mod_files mf
                 JOIN mods m ON m.id = mf.mod_id
                 LEFT JOIN profile_mods pm ON pm.mod_id = m.id AND pm.profile_id = ?2
                 WHERE m.instance_id = ?1
                   AND COALESCE(pm.enabled, m.enabled_default) = 1
                   AND mf.abs_source_path = ?3
                 LIMIT 1",
                rusqlite::params![instance_id, profile_id, path.to_string_lossy()],
                |row| row.get(0),
            )
            .map_err(|_| {
                crate::error::SlimError::Safety(format!(
                    "Werkzeug liegt weder im Spielverzeichnis noch in einem aktiven Mod: {}",
                    path.display()
                ))
            })?;
        normalized_rel_path
            .strip_prefix(crate::scanner::GAME_ROOT_PREFIX)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data").join(normalized_rel_path))
    };
    Ok(resolve_case_insensitive_path(mount_path, &relative)
        .unwrap_or_else(|| mount_path.join(relative)))
}

fn resolve_case_insensitive_path(root: &Path, relative: &Path) -> Option<PathBuf> {
    let components = relative
        .components()
        .map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_os_string()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;

    fn resolve_from(current: &Path, components: &[std::ffi::OsString]) -> Option<PathBuf> {
        let Some((expected, remaining)) = components.split_first() else {
            return Some(current.to_path_buf());
        };
        let expected_text = expected.to_string_lossy();
        let mut matches = std::fs::read_dir(current)
            .ok()?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&expected_text)
            })
            .collect::<Vec<_>>();
        // Overlay mounts can contain legacy paths that differ only by case. Prefer the
        // exact spelling, but backtrack into every matching branch if it is incomplete.
        matches.sort_by_key(|entry| entry.file_name() != *expected);
        for entry in matches {
            if let Some(path) = resolve_from(&current.join(entry.file_name()), remaining) {
                return Some(path);
            }
        }
        None
    }

    resolve_from(root, &components)
}

#[tauri::command]
pub fn validate_tool_profiles(state: State<AppState>) -> SlimResult<Vec<ToolProfileValidation>> {
    state.with_connection(tools::validate_tool_profiles)
}

#[tauri::command]
pub fn list_recent_tool_runs(state: State<AppState>) -> SlimResult<Vec<ToolRunRecord>> {
    state.with_connection(|conn| tools::list_recent_tool_runs(conn, 20))
}

#[tauri::command]
pub fn scan_mod_files(request: ScanModFilesRequest) -> SlimResult<Vec<ModFile>> {
    mods::scan_mod_files(&request.mod_id, &request.source_path)
}

#[tauri::command]
pub fn build_dry_run_plan(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<DeployPlan> {
    let workspace_root = state.workspace_root.clone();
    state.with_connection(|conn| {
        deploy::build_plan(
            conn,
            &workspace_root,
            &instance_id,
            &profile_id,
            DeployTarget::DryRun,
            DeployAction::Copy,
        )
    })
}

#[tauri::command]
pub fn execute_staging_plan(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<DeployPlan> {
    let workspace_root = state.workspace_root.clone();
    let plan = state.with_connection(|conn| {
        deploy::build_plan(
            conn,
            &workspace_root,
            &instance_id,
            &profile_id,
            DeployTarget::Staging,
            DeployAction::Copy,
        )
    })?;
    staging::execute_staging_plan(&workspace_root, &plan)?;
    Ok(plan)
}

#[tauri::command]
pub fn execute_real_deploy_plan(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
    action: DeployAction,
) -> SlimResult<DeployPlan> {
    let _ = (state, instance_id, profile_id, action);
    Err(crate::error::SlimError::Disabled(
        "Direct Data deployment is disabled in SLiM-CC v2; use the profile VFS".into(),
    ))
}

#[tauri::command]
pub fn list_backup_snapshots(
    state: State<AppState>,
    instance_id: String,
) -> SlimResult<Vec<crate::models::BackupSnapshot>> {
    let workspace_root = state.workspace_root.clone();
    let backups_root = crate::paths::backups_root(&workspace_root, &instance_id);
    crate::backup::list_backup_snapshots(&backups_root, &instance_id)
}

#[tauri::command]
pub fn restore_backup_snapshot(
    state: State<AppState>,
    instance_id: String,
    snapshot_id: String,
) -> SlimResult<()> {
    let workspace_root = state.workspace_root.clone();
    let instance =
        state.with_connection(|conn| crate::instance::get_instance_by_id(conn, &instance_id))?;
    let snapshot_root =
        crate::paths::backups_root(&workspace_root, &instance_id).join(&snapshot_id);
    let snapshot = crate::backup::read_backup_snapshot(&snapshot_root)?;
    if snapshot.instance_id != instance_id {
        return Err(crate::error::SlimError::Safety(
            "snapshot instance mismatch".into(),
        ));
    }
    if snapshot.source_root != instance.data_path {
        return Err(crate::error::SlimError::Safety(format!(
            "snapshot source root does not match active instance data path: {} != {}",
            snapshot.source_root.display(),
            instance.data_path.display()
        )));
    }
    let backups_root = crate::paths::backups_root(&workspace_root, &instance_id);
    let _rollback_snapshot = crate::backup::create_backup_snapshot(
        &backups_root,
        &instance_id,
        None,
        &instance.data_path,
        "pre-restore",
    )?;
    crate::backup::restore_backup_snapshot(&snapshot, &instance.data_path)
}

#[tauri::command]
pub fn list_conflicts(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<Vec<crate::models::ConflictEntry>> {
    let workspace_root = state.workspace_root.clone();
    state.with_connection(|conn| {
        conflicts::list_conflicts(conn, &workspace_root, &instance_id, &profile_id)
    })
}

#[tauri::command]
pub fn list_mod_conflict_summary(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<Vec<ModConflictSummary>> {
    let workspace_root = state.workspace_root.clone();
    state.with_connection(|conn| {
        conflicts::list_mod_conflict_summary(conn, &workspace_root, &instance_id, &profile_id)
    })
}

#[tauri::command]
pub fn build_diagnosis_report(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<DiagnosisReport> {
    let workspace_root = state.workspace_root.clone();
    state.with_connection(|conn| {
        crate::diagnostics::build_diagnosis_report(conn, &workspace_root, &instance_id, &profile_id)
    })
}

#[tauri::command]
pub fn import_diagnostic_logs(
    state: State<AppState>,
    log_dir: String,
    instance_id: Option<String>,
) -> SlimResult<Vec<StoredDiagnosticFinding>> {
    state.with_connection(|conn| {
        crate::diagnostics::import_and_store_diagnostic_logs(
            conn,
            instance_id.as_deref(),
            Path::new(&log_dir),
        )
    })
}

#[tauri::command]
pub fn compare_profiles(
    state: State<AppState>,
    instance_id: String,
    base_profile_id: String,
    compare_profile_id: String,
) -> SlimResult<ProfileComparison> {
    state.with_connection(|conn| {
        crate::diagnostics::compare_profiles(
            conn,
            &instance_id,
            &base_profile_id,
            &compare_profile_id,
        )
    })
}

#[tauri::command]
pub fn pick_directory(title: Option<String>, default_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();

    if let Some(title) = title {
        dialog = dialog.set_title(&title);
    }

    if let Some(default_path) = default_path {
        let path = PathBuf::from(default_path);
        if path.exists() {
            dialog = dialog.set_directory(path);
        }
    }

    dialog.pick_folder().map(|path| path.display().to_string())
}

#[tauri::command]
pub fn pick_file(title: Option<String>, default_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();

    if let Some(title) = title {
        dialog = dialog.set_title(&title);
    }

    if let Some(default_path) = default_path {
        let path = PathBuf::from(default_path);
        if path.exists() {
            if path.is_dir() {
                dialog = dialog.set_directory(path);
            } else {
                dialog = dialog.set_directory(path.parent()?.to_path_buf());
            }
        }
    }

    dialog.pick_file().map(|path| path.display().to_string())
}

#[tauri::command]
pub fn pick_save_file(title: Option<String>, default_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();

    if let Some(title) = title {
        dialog = dialog.set_title(&title);
    }

    if let Some(default_path) = default_path {
        let path = PathBuf::from(default_path);
        if path.exists() {
            if path.is_dir() {
                dialog = dialog.set_directory(path);
            } else if let Some(parent) = path.parent() {
                dialog = dialog.set_directory(parent.to_path_buf());
            }
        } else if let Some(parent) = path.parent().filter(|parent| parent.exists()) {
            dialog = dialog.set_directory(parent.to_path_buf());
            if let Some(file_name) = path.file_name().and_then(|value| value.to_str()) {
                dialog = dialog.set_file_name(file_name);
            }
        }
    }

    dialog.save_file().map(|path| path.display().to_string())
}

#[tauri::command]
pub fn get_app_settings(state: State<AppState>) -> SlimResult<AppSettings> {
    state.with_connection(settings::load_settings)
}

#[tauri::command]
pub fn update_app_settings(
    state: State<AppState>,
    request: UpdateAppSettingsRequest,
) -> SlimResult<AppSettings> {
    state.with_connection_mut(|conn| settings::save_settings(conn, request))
}

#[tauri::command]
pub fn update_nexus_api_key(
    state: State<AppState>,
    request: UpdateNexusApiKeyRequest,
) -> SlimResult<AppSettings> {
    state.with_connection_mut(|conn| settings::save_nexus_api_key(conn, request))
}

#[tauri::command]
pub fn list_mod_download_candidates(
    state: State<AppState>,
    instance_id: Option<String>,
) -> SlimResult<Vec<ModDownloadCandidate>> {
    state.with_connection(|conn| {
        settings::list_mod_download_candidates(conn, instance_id.as_deref())
    })
}

#[tauri::command]
pub fn open_external_url(app: AppHandle, url: String) -> SlimResult<()> {
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|error| crate::error::SlimError::Process(error.to_string()))
}

#[tauri::command]
pub fn preview_loot_sort(
    state: State<AppState>,
    instance_id: String,
    profile_id: String,
) -> SlimResult<LootSortPreview> {
    let loot_executable_path = state.with_connection(resolve_loot_executable_path)?;
    let workspace_root = state.workspace_root.clone();
    state.with_connection(|conn| {
        mods::preview_loot_sort(
            conn,
            &workspace_root,
            loot_executable_path,
            &instance_id,
            &profile_id,
        )
    })
}

#[tauri::command]
pub fn launch_loot(
    state: State<AppState>,
    request: LaunchLootRequest,
) -> SlimResult<LootLaunchResult> {
    let vfs_status = prepare_profile_vfs(
        state.clone(),
        request.instance_id.clone(),
        request.profile_id.clone(),
    )?;
    let loot_executable_path = state
        .with_connection(resolve_loot_executable_path)?
        .ok_or_else(|| {
            crate::error::SlimError::InvalidPath("LOOT executable is not configured".into())
        })?;
    let workspace_root = state.workspace_root.clone();
    state.with_connection_mut(|conn| {
        mods::launch_loot(
            conn,
            &workspace_root,
            &loot_executable_path,
            &request.instance_id,
            &request.profile_id,
            &vfs_status.mount_path,
        )
    })
}

fn resolve_loot_executable_path(conn: &rusqlite::Connection) -> SlimResult<Option<PathBuf>> {
    let tool_path = tools::get_tool_profile(conn, "loot")
        .ok()
        .and_then(|profile| profile.executable_path);
    if tool_path
        .as_ref()
        .is_some_and(|path| is_non_empty_path(path))
    {
        return Ok(tool_path);
    }

    Ok(settings::load_settings(conn)?.loot_executable_path)
}

fn is_non_empty_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
}

#[cfg(test)]
mod tests {
    use super::resolve_case_insensitive_path;
    use std::fs;
    use std::path::Path;
    use uuid::Uuid;

    #[test]
    fn vfs_tool_paths_resolve_case_insensitively() {
        let root = std::env::temp_dir().join(format!("slimcc-vfs-case-{}", Uuid::new_v4()));
        let executable = root.join("Data/Tools/FNIS/GenerateFNIS.exe");
        fs::create_dir_all(executable.parent().unwrap()).expect("create tool path");
        fs::write(&executable, b"exe").expect("write executable");

        assert_eq!(
            resolve_case_insensitive_path(&root, Path::new("data/tools/fnis/generatefnis.exe")),
            Some(executable)
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn vfs_tool_paths_backtrack_across_case_collisions() {
        let root = std::env::temp_dir().join(format!("slimcc-vfs-collision-{}", Uuid::new_v4()));
        let legacy_directory = root.join("Data/tools/GenerateFNIS_for_Users");
        let executable = root.join("Data/tools/generatefnis_for_users/generatefnisforusers.exe");
        fs::create_dir_all(&legacy_directory).expect("create incomplete legacy path");
        fs::create_dir_all(executable.parent().unwrap()).expect("create normalized tool path");
        fs::write(&executable, b"exe").expect("write executable");

        assert_eq!(
            resolve_case_insensitive_path(
                &root,
                Path::new("data/tools/generatefnis_for_users/generatefnisforusers.exe")
            ),
            Some(executable)
        );

        let _ = fs::remove_dir_all(root);
    }
}
