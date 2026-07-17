use crate::error::{SlimError, SlimResult};
use crate::fomod::{self, FomodDependencyContext};
use crate::models::{
    FomodDependency, FomodDependencyGroup, FomodPackagePreview, ImportModFolderRequest,
    ImportedModReport, LootLaunchResult, LootSortPreview, ModDependencyStatus,
    ModDependencySummary, ModFile, ModRecord, ProfileModEntry, ProfilePluginEntry,
    UpdateModDependenciesRequest, UpdateModRequest, UpdateProfileModsRequest,
    UpdateProfilePluginsRequest,
};
use crate::paths;
use crate::scanner;
use crate::tools::{self, RunnerType, ToolLaunchRequest};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Component;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FomodInstallerState {
    source_root: PathBuf,
    selection: crate::models::FomodSelectionRequest,
}

pub fn scan_mod_files(mod_id: &str, source_path: &Path) -> SlimResult<Vec<ModFile>> {
    scanner::scan_mod_files(mod_id, source_path)
}

pub fn update_mod(conn: &mut Connection, request: UpdateModRequest) -> SlimResult<ModRecord> {
    let now = Utc::now().to_rfc3339();
    let tags_json = serde_json::to_string(&request.tags)?;
    let tx = conn.transaction()?;
    let updated = tx.execute(
        "UPDATE mods
         SET name = ?2,
             enabled_default = ?3,
             updated_at = ?4
         WHERE id = ?1",
        params![
            &request.id,
            &request.name,
            if request.enabled_default { 1 } else { 0 },
            &now
        ],
    )?;

    if updated == 0 {
        return Err(crate::error::SlimError::NotFound(format!(
            "mod not found: {}",
            request.id
        )));
    }

    tx.execute(
        "INSERT INTO mod_editor_state (mod_id, tags_json, notes, rule_type, rule_target_mod_id, rule_weight, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(mod_id) DO UPDATE SET
             tags_json = excluded.tags_json,
             notes = excluded.notes,
             rule_type = excluded.rule_type,
             rule_target_mod_id = excluded.rule_target_mod_id,
             rule_weight = excluded.rule_weight,
             updated_at = excluded.updated_at",
        params![
            &request.id,
            tags_json,
            request.notes,
            request.rule_type,
            request.rule_target_mod_id,
            request.rule_weight,
            &now
        ],
    )?;

    tx.commit()?;
    crate::workspace::get_mod_by_id(conn, &request.id)
}

pub fn delete_mod(conn: &mut Connection, mod_id: &str) -> SlimResult<()> {
    let tx = conn.transaction()?;
    let deleted = tx.execute("DELETE FROM mods WHERE id = ?1", params![mod_id])?;
    if deleted == 0 {
        return Err(crate::error::SlimError::NotFound(format!(
            "mod not found: {mod_id}"
        )));
    }
    tx.commit()?;
    Ok(())
}

pub fn list_profile_mods(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<Vec<ProfileModEntry>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.name, m.version, m.source_path, m.installed_path,
                COALESCE(pm.enabled, m.enabled_default),
                COALESCE(pm.priority, 0),
                (SELECT COUNT(1) FROM plugins p WHERE p.mod_id = m.id) AS plugin_count,
                COALESCE(mes.rule_type, 'none'),
                mes.rule_target_mod_id,
                COALESCE(mes.rule_weight, 0)
         FROM mods m
         LEFT JOIN profile_mods pm ON pm.mod_id = m.id AND pm.profile_id = ?2
         LEFT JOIN mod_editor_state mes ON mes.mod_id = m.id
         WHERE m.instance_id = ?1
         ORDER BY COALESCE(pm.priority, 0) DESC, m.created_at DESC",
    )?;

    let rows = stmt.query_map(params![instance_id, profile_id], |row| {
        Ok(ProfileModEntry {
            mod_id: row.get(0)?,
            mod_name: row.get(1)?,
            version: row.get(2)?,
            source_path: std::path::PathBuf::from(row.get::<_, String>(3)?),
            installed_path: std::path::PathBuf::from(row.get::<_, String>(4)?),
            enabled: row.get::<_, i64>(5)? != 0,
            priority: row.get(6)?,
            plugin_count: row.get(7)?,
            rule_type: {
                let rule_type: String = row.get(8)?;
                if rule_type == "none" {
                    None
                } else {
                    Some(rule_type)
                }
            },
            rule_target_mod_id: row.get(9)?,
            rule_weight: row.get(10)?,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn update_profile_mods(
    conn: &mut Connection,
    request: UpdateProfileModsRequest,
) -> SlimResult<()> {
    let tx = conn.transaction()?;
    for entry in request.entries {
        tx.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(profile_id, mod_id) DO UPDATE SET
                 enabled = excluded.enabled,
                 priority = excluded.priority",
            params![
                &request.profile_id,
                &entry.mod_id,
                if entry.enabled { 1 } else { 0 },
                entry.priority
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn list_profile_plugins(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<Vec<ProfilePluginEntry>> {
    ensure_plugin_index_for_instance(conn, instance_id)?;

    let mut stmt = conn.prepare(
        "SELECT p.id,
                p.mod_id,
                m.name,
                p.filename,
                p.plugin_type,
                p.normalized_rel_path,
                COALESCE(pm.enabled, m.enabled_default),
                COALESCE(pp.enabled, 1),
                COALESCE(pm.priority, 0)
         FROM plugins p
         JOIN mods m ON m.id = p.mod_id
         LEFT JOIN profile_mods pm ON pm.mod_id = m.id AND pm.profile_id = ?2
         LEFT JOIN profile_plugins pp ON pp.plugin_id = p.id AND pp.profile_id = ?2
         WHERE m.instance_id = ?1
           AND COALESCE(pm.enabled, m.enabled_default) = 1
         ORDER BY COALESCE(pm.priority, 0) ASC, LOWER(p.filename) ASC",
    )?;

    let rows = stmt.query_map(params![instance_id, profile_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)? != 0,
            row.get::<_, i64>(7)? != 0,
            row.get::<_, i64>(8)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (
            plugin_id,
            mod_id,
            mod_name,
            filename,
            plugin_type,
            normalized_rel_path,
            mod_enabled,
            enabled,
            priority,
        ) = row?;
        let summary = dependency_summary_for_mod(conn, &mod_id)?;
        out.push(ProfilePluginEntry {
            plugin_id,
            mod_id,
            mod_name,
            filename,
            plugin_type,
            normalized_rel_path,
            mod_enabled,
            enabled,
            priority,
            dependency_count: summary.dependency_count,
            missing_dependency_count: summary.missing_count,
            dependency_status: summary.status,
        });
    }
    Ok(out)
}

pub fn update_profile_plugins(
    conn: &mut Connection,
    request: UpdateProfilePluginsRequest,
) -> SlimResult<Vec<ProfilePluginEntry>> {
    let instance_id = profile_instance_id(conn, &request.profile_id)?;
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;
    for entry in request.entries {
        let plugin_instance_id = plugin_instance_id(&tx, &entry.plugin_id)?;
        if plugin_instance_id != instance_id {
            return Err(SlimError::InvalidPath(format!(
                "plugin is in a different instance: {}",
                entry.plugin_id
            )));
        }
        if entry.enabled {
            tx.execute(
                "DELETE FROM profile_plugins WHERE profile_id = ?1 AND plugin_id = ?2",
                params![&request.profile_id, &entry.plugin_id],
            )?;
        } else {
            tx.execute(
                "INSERT INTO profile_plugins (profile_id, plugin_id, enabled, updated_at)
                 VALUES (?1, ?2, 0, ?3)
                 ON CONFLICT(profile_id, plugin_id) DO UPDATE SET
                     enabled = excluded.enabled,
                     updated_at = excluded.updated_at",
                params![&request.profile_id, &entry.plugin_id, &now],
            )?;
        }
    }
    tx.commit()?;

    list_profile_plugins(conn, &instance_id, &request.profile_id)
}

pub fn list_mod_dependencies(
    conn: &Connection,
    mod_id: &str,
) -> SlimResult<Vec<ModDependencyStatus>> {
    list_mod_dependencies_for_profile(conn, mod_id, None)
}

pub fn list_mod_dependencies_for_profile(
    conn: &Connection,
    mod_id: &str,
    profile_id: Option<&str>,
) -> SlimResult<Vec<ModDependencyStatus>> {
    ensure_dependency_state_column(conn)?;
    let instance_id = mod_instance_id(conn, mod_id)?;
    let mut stmt = conn.prepare(
        "SELECT id, mod_id, dependency_type, target_mod_id, target_value, notes, satisfied_override
         FROM mod_dependencies
         WHERE mod_id = ?1
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![mod_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<i64>>(6)?,
        ))
    })?;

    let mut dependencies = Vec::new();
    for row in rows {
        let (id, mod_id, dependency_type, target_mod_id, target_value, notes, satisfied_override) =
            row?;
        let satisfied = if dependency_type == "manual" {
            satisfied_override == Some(1)
        } else {
            dependency_satisfied_for_profile(
                conn,
                &instance_id,
                profile_id,
                &dependency_type,
                target_mod_id.as_deref(),
                &target_value,
            )?
        };
        dependencies.push(ModDependencyStatus {
            id,
            mod_id,
            dependency_type,
            target_mod_id,
            target_value,
            notes,
            satisfied,
            status: if satisfied { "satisfied" } else { "missing" }.into(),
        });
    }

    Ok(dependencies)
}

pub fn set_manual_dependency_satisfied(
    conn: &Connection,
    dependency_id: &str,
    satisfied: bool,
) -> SlimResult<()> {
    ensure_dependency_state_column(conn)?;
    let changed = conn.execute(
        "UPDATE mod_dependencies SET satisfied_override = ?2, updated_at = ?3 WHERE id = ?1 AND dependency_type = 'manual'",
        params![dependency_id, if satisfied { 1 } else { 0 }, Utc::now().to_rfc3339()],
    )?;
    if changed == 0 {
        return Err(SlimError::NotFound(format!(
            "manual dependency not found: {dependency_id}"
        )));
    }
    Ok(())
}

fn ensure_dependency_state_column(conn: &Connection) -> SlimResult<()> {
    let has_column = conn
        .prepare("PRAGMA table_info(mod_dependencies)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == "satisfied_override");
    if !has_column {
        conn.execute(
            "ALTER TABLE mod_dependencies ADD COLUMN satisfied_override INTEGER",
            [],
        )?;
    }
    Ok(())
}

pub fn list_instance_dependency_summary(
    conn: &Connection,
    instance_id: &str,
) -> SlimResult<Vec<ModDependencySummary>> {
    list_instance_dependency_summary_for_profile(conn, instance_id, None)
}

pub fn list_instance_dependency_summary_for_profile(
    conn: &Connection,
    instance_id: &str,
    profile_id: Option<&str>,
) -> SlimResult<Vec<ModDependencySummary>> {
    ensure_plugin_index_for_instance(conn, instance_id)?;

    let mut stmt = conn.prepare(
        "SELECT id
         FROM mods
         WHERE instance_id = ?1
         ORDER BY created_at DESC, id ASC",
    )?;
    let rows = stmt.query_map(params![instance_id], |row| row.get::<_, String>(0))?;

    let mut summaries = Vec::new();
    for row in rows {
        let mod_id = row?;
        let dependencies = list_mod_dependencies_for_profile(conn, &mod_id, profile_id)?;
        let dependency_count = dependencies.len() as i64;
        let missing_count = dependencies
            .iter()
            .filter(|dependency| !dependency.satisfied)
            .count() as i64;
        summaries.push(ModDependencySummary {
            mod_id,
            dependency_count,
            missing_count,
            status: if missing_count == 0 { "ok" } else { "missing" }.into(),
        });
    }

    Ok(summaries)
}

fn dependency_summary_for_mod(conn: &Connection, mod_id: &str) -> SlimResult<ModDependencySummary> {
    let dependencies = list_mod_dependencies(conn, mod_id)?;
    let dependency_count = dependencies.len() as i64;
    let missing_count = dependencies
        .iter()
        .filter(|dependency| !dependency.satisfied)
        .count() as i64;
    Ok(ModDependencySummary {
        mod_id: mod_id.to_string(),
        dependency_count,
        missing_count,
        status: if missing_count == 0 { "ok" } else { "missing" }.into(),
    })
}

pub fn update_mod_dependencies(
    conn: &mut Connection,
    request: UpdateModDependenciesRequest,
) -> SlimResult<Vec<ModDependencyStatus>> {
    let instance_id = mod_instance_id(conn, &request.mod_id)?;
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM mod_dependencies WHERE mod_id = ?1",
        params![&request.mod_id],
    )?;

    for entry in request.entries {
        let dependency_type = normalize_dependency_type(&entry.dependency_type)?;
        let target_value = entry.target_value.trim();
        if target_value.is_empty() && entry.target_mod_id.is_none() {
            continue;
        }
        if let Some(target_mod_id) = entry.target_mod_id.as_deref() {
            let target_instance_id = mod_instance_id(&tx, target_mod_id)?;
            if target_instance_id != instance_id {
                return Err(SlimError::InvalidPath(format!(
                    "dependency target is in a different instance: {target_mod_id}"
                )));
            }
        }
        tx.execute(
            "INSERT INTO mod_dependencies (
                 id, mod_id, dependency_type, target_mod_id, target_value, notes, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                Uuid::new_v4().to_string(),
                &request.mod_id,
                dependency_type,
                entry.target_mod_id,
                target_value,
                entry.notes.trim(),
                &now
            ],
        )?;
    }
    tx.commit()?;

    list_mod_dependencies(conn, &request.mod_id)
}

pub fn preview_loot_sort(
    conn: &Connection,
    workspace_root: &Path,
    loot_executable_path: Option<std::path::PathBuf>,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<LootSortPreview> {
    let current_order = list_profile_mods(conn, instance_id, profile_id)?;
    let mut notes = Vec::new();
    let mut proposed_order = current_order.clone();

    if let Some(loot_executable_path) = loot_executable_path.clone() {
        let instance = crate::instance::get_instance_by_id(conn, instance_id)?;
        let game_identifier = loot_game_identifier(&instance.game_type).ok_or_else(|| {
            SlimError::InvalidPath(format!(
                "unsupported game type for LOOT preview: {}",
                instance.game_type
            ))
        })?;
        let preview_game_root =
            paths::instance_root(workspace_root, instance_id).join("loot-preview");
        let preview_data_root = preview_game_root.join("Data");
        let plan = crate::deploy::build_plan(
            conn,
            workspace_root,
            instance_id,
            profile_id,
            crate::models::DeployTarget::Staging,
            crate::models::DeployAction::Copy,
        )?;
        crate::staging::execute_deploy_plan_at_root(
            workspace_root,
            &plan,
            &paths::staging_data_path(workspace_root, instance_id),
        )?;
        if preview_data_root.exists() {
            paths::guard_descendant(&preview_data_root, &preview_data_root.join(".guard"))?;
            fs::remove_dir_all(&preview_data_root)?;
        }
        fs::create_dir_all(&preview_game_root)?;
        copy_directory_recursive(
            &paths::staging_data_path(workspace_root, instance_id),
            &preview_data_root,
        )?;

        let launch_context = prepare_loot_launch_context_with_data_root(
            &paths::instance_root(workspace_root, instance_id).join("loot-preview-data"),
            game_identifier,
            &preview_game_root,
        )?;
        seed_loot_local_files(
            conn,
            instance_id,
            profile_id,
            &launch_context.game_local_path,
        )?;
        let tool_request = build_loot_tool_launch_request(
            conn,
            &loot_executable_path,
            &launch_context.args,
            &preview_game_root,
            instance.wine_prefix.as_deref(),
        )?;
        if !tool_request.executable_path.exists() {
            return Err(SlimError::NotFound(format!(
                "LOOT executable not found: {}",
                tool_request.executable_path.display()
            )));
        }
        let (_process_id, execution) = tools::launch_tool(&tool_request)?;
        if execution.exit_code != Some(0) {
            let mut message = format!("LOOT preview exited with status {:?}", execution.exit_code);
            if let Some(stderr) = execution
                .stderr
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                message.push('\n');
                message.push_str(stderr);
            }
            return Err(SlimError::Process(message));
        }
        proposed_order = preview_loot_order_from_files(
            conn,
            instance_id,
            profile_id,
            &launch_context.game_local_path,
        )?;
        notes.push("LOOT preview ran against an instance-local staged Data tree.".into());
    } else {
        notes.push("Configure LOOT executable to run a live preview.".into());
    }

    Ok(LootSortPreview {
        instance_id: instance_id.to_string(),
        profile_id: profile_id.to_string(),
        loot_executable_path,
        notes,
        current_order,
        proposed_order,
    })
}

pub fn launch_loot(
    conn: &mut Connection,
    workspace_root: &Path,
    loot_executable_path: &Path,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<LootLaunchResult> {
    let instance = crate::instance::get_instance_by_id(conn, instance_id)?;
    let game_identifier = loot_game_identifier(&instance.game_type).ok_or_else(|| {
        SlimError::InvalidPath(format!(
            "unsupported game type for LOOT launch: {}",
            instance.game_type
        ))
    })?;

    let launch_context = prepare_loot_launch_context(
        workspace_root,
        instance_id,
        game_identifier,
        &instance.install_path,
    )?;
    seed_loot_local_files(
        conn,
        instance_id,
        profile_id,
        &launch_context.game_local_path,
    )?;

    let tool_request = build_loot_tool_launch_request(
        conn,
        loot_executable_path,
        &launch_context.args,
        &instance.install_path,
        instance.wine_prefix.as_deref(),
    )?;
    if !tool_request.executable_path.exists() {
        return Err(SlimError::NotFound(format!(
            "LOOT executable not found: {}",
            tool_request.executable_path.display()
        )));
    }
    let started_at = Utc::now().to_rfc3339();
    let preview = tools::build_command_preview(&tool_request)?;
    let (process_id, execution) = tools::launch_tool(&tool_request)?;
    let finished_at = Utc::now().to_rfc3339();
    tools::record_tool_run(
        conn,
        "loot",
        "LOOT",
        &preview,
        &execution,
        &started_at,
        &finished_at,
    )?;
    if execution.exit_code != Some(0) {
        let mut message = format!("LOOT exited with status {:?}", execution.exit_code);
        if let Some(stderr) = execution
            .stderr
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            message.push('\n');
            message.push_str(stderr);
        }
        return Err(SlimError::Process(message));
    }

    let imported_order = import_loot_order(
        conn,
        instance_id,
        profile_id,
        &launch_context.game_local_path,
    )?;

    Ok(LootLaunchResult {
        instance_id: instance_id.to_string(),
        game_identifier: game_identifier.to_string(),
        executable_path: tool_request.executable_path,
        args: launch_context.args,
        process_id,
        synced_mods: imported_order,
        exit_code: execution.exit_code,
        stdout: execution.stdout,
        stderr: execution.stderr,
    })
}

fn build_loot_tool_launch_request(
    conn: &Connection,
    fallback_executable_path: &Path,
    launch_args: &[String],
    install_path: &Path,
    instance_wine_prefix: Option<&Path>,
) -> SlimResult<ToolLaunchRequest> {
    let mut settings = crate::settings::load_settings(conn)?;
    settings.install_path = Some(install_path.to_path_buf());
    if settings.wine_prefix.is_none() {
        settings.wine_prefix = instance_wine_prefix.map(PathBuf::from);
    }

    if let Ok(profile) = tools::get_tool_profile(conn, "loot") {
        if let Some(executable_path) = profile
            .executable_path
            .clone()
            .filter(|path| !path.as_os_str().is_empty())
        {
            return tools::build_tool_launch_request_with_arguments(
                &profile,
                executable_path,
                &settings,
                launch_args.to_vec(),
            );
        }
    }

    Ok(ToolLaunchRequest {
        runner_type: RunnerType::Native,
        executable_path: fallback_executable_path.to_path_buf(),
        arguments: launch_args.to_vec(),
        working_directory: Some(install_path.to_path_buf()),
        wine_prefix: instance_wine_prefix.map(PathBuf::from),
    })
}

pub fn import_mod_folder(
    conn: &mut Connection,
    workspace_root: &Path,
    request: ImportModFolderRequest,
) -> SlimResult<ImportedModReport> {
    let instance_id = request.instance_id.clone();
    let name = request.name.clone();
    let source_path = request.source_path.clone();
    let instance_mods_root = paths::mods_root(workspace_root, &instance_id);
    fs::create_dir_all(&instance_mods_root)?;

    let reconfigure = request.target_mod_id.is_some();
    let mod_id = request
        .target_mod_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if reconfigure {
        let existing_instance: String = conn
            .query_row(
                "SELECT instance_id FROM mods WHERE id=?1",
                params![&mod_id],
                |row| row.get(0),
            )
            .map_err(|_| SlimError::NotFound(format!("mod not found: {mod_id}")))?;
        if existing_instance != instance_id {
            return Err(SlimError::Safety(
                "cannot reconfigure a mod from another instance".into(),
            ));
        }
    }

    let import_key = format!("{mod_id}-{}", Uuid::new_v4());
    let (package_source_path, archive_forced_workspace, transient_source) =
        if let Some(preview_source_path) = request.preview_source_path.as_deref() {
            let validated = validate_preview_source(workspace_root, preview_source_path)?;
            (validated, true, true)
        } else {
            let (prepared, forced) =
                prepare_import_source(&source_path, &instance_mods_root, &import_key)?;
            (prepared, forced, source_path.is_file())
        };
    let copy_into_workspace =
        request.copy_into_workspace || archive_forced_workspace || reconfigure;
    let dependency_context = build_fomod_dependency_context_for_profile(
        conn,
        &instance_id,
        request.profile_id.as_deref(),
    )?;
    let preview =
        fomod::inspect_package_with_context(&package_source_path, None, Some(&dependency_context))?;
    let fomod_source_root = preview.source_path.clone();
    if !fomod::module_dependencies_satisfied(&preview, Some(&dependency_context)) {
        if transient_source {
            let _ = fs::remove_dir_all(&package_source_path);
        }
        return Err(SlimError::Safety(
            "FOMOD module dependencies are not satisfied".into(),
        ));
    }

    let final_target = instance_mods_root.join(&mod_id);
    let temporary_target = instance_mods_root.join(format!(".{import_key}.installing"));
    let backup_target = instance_mods_root.join(format!(".{import_key}.backup"));
    let managed_install = preview.has_fomod || copy_into_workspace;
    if managed_install {
        fs::create_dir_all(&temporary_target)?;
        paths::guard_descendant(&instance_mods_root, &temporary_target.join(".guard"))?;
    }

    let resolved_selection = if preview.has_fomod {
        if !copy_into_workspace {
            return Err(SlimError::InvalidPath(
                "FOMOD installs must be copied into the workspace".into(),
            ));
        }
        let resolved_selection = fomod::merge_selection_with_context(
            &preview,
            request.fomod_selection.clone(),
            Some(&dependency_context),
        );
        if let Err(error) = fomod::install_selected_files_with_context(
            &fomod_source_root,
            &temporary_target,
            &preview,
            &resolved_selection,
            Some(&dependency_context),
        ) {
            let _ = fs::remove_dir_all(&temporary_target);
            if transient_source {
                let _ = fs::remove_dir_all(&package_source_path);
            }
            return Err(error);
        }
        Some(resolved_selection)
    } else {
        if copy_into_workspace {
            if let Err(error) = copy_directory_recursive(&package_source_path, &temporary_target) {
                let _ = fs::remove_dir_all(&temporary_target);
                if transient_source {
                    let _ = fs::remove_dir_all(&package_source_path);
                }
                return Err(error);
            }
        }
        None
    };

    let mut replaced_existing = false;
    let installed_path = if managed_install {
        if final_target.exists() {
            fs::rename(&final_target, &backup_target)?;
            replaced_existing = true;
        }
        if let Err(error) = fs::rename(&temporary_target, &final_target) {
            if replaced_existing {
                let _ = fs::rename(&backup_target, &final_target);
            }
            return Err(error.into());
        }
        final_target.clone()
    } else {
        package_source_path.clone()
    };
    let files = match scanner::scan_mod_files(&mod_id, &installed_path) {
        Ok(files) => files,
        Err(error) => {
            if managed_install {
                let _ = fs::remove_dir_all(&final_target);
            }
            if replaced_existing {
                let _ = fs::rename(&backup_target, &final_target);
            }
            return Err(error);
        }
    };
    let installer_state_json = resolved_selection
        .as_ref()
        .map(|selection| {
            serde_json::to_string(&FomodInstallerState {
                source_root: source_path.clone(),
                selection: selection.clone(),
            })
        })
        .transpose()?;

    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;
    let persistence_result: SlimResult<()> = (|| {
        if reconfigure {
            tx.execute("DELETE FROM plugins WHERE mod_id=?1", params![&mod_id])?;
            tx.execute("DELETE FROM mod_files WHERE mod_id=?1", params![&mod_id])?;
            tx.execute(
                "DELETE FROM mod_installers WHERE mod_id=?1",
                params![&mod_id],
            )?;
            tx.execute(
                "UPDATE mods SET name=?2, source_path=?3, installed_path=?4, updated_at=?5 WHERE id=?1",
                params![&mod_id, &name, source_path.to_string_lossy().to_string(), installed_path.to_string_lossy().to_string(), &now],
            )?;
        } else {
            tx.execute(
                "INSERT INTO mods (id, instance_id, name, version, source_path, installed_path, enabled_default, created_at, updated_at)
                 VALUES (?1, ?2, ?3, NULL, ?4, ?5, 1, ?6, ?6)",
                params![&mod_id, &instance_id, &name, source_path.to_string_lossy().to_string(), installed_path.to_string_lossy().to_string(), &now],
            )?;
        }

        for file in &files {
            tx.execute(
            "INSERT INTO mod_files (id, mod_id, original_rel_path, normalized_rel_path, abs_source_path, file_size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                &file.mod_id,
                &file.original_rel_path,
                &file.normalized_rel_path,
                file.abs_source_path.to_string_lossy().to_string(),
                file.file_size.map(|size| size as i64),
                &now
            ],
            )?;

            if scanner::is_plugin_path(&file.original_rel_path) {
                let filename = PathBuf::from(&file.original_rel_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&file.original_rel_path)
                    .to_string();
                let plugin_type = Path::new(&filename)
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                tx.execute(
                "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    Uuid::new_v4().to_string(),
                    &file.mod_id,
                    filename,
                    plugin_type,
                    &file.normalized_rel_path,
                    &now
                ],
                )?;
            }
        }

        if let Some(state_json) = installer_state_json.as_deref() {
            tx.execute(
                "INSERT INTO mod_installers (mod_id, installer_type, state_json, updated_at)
             VALUES (?1, 'fomod', ?2, ?3)
             ON CONFLICT(mod_id) DO UPDATE SET
                 installer_type = excluded.installer_type,
                 state_json = excluded.state_json,
                 updated_at = excluded.updated_at",
                params![&mod_id, state_json, &now],
            )?;
        }
        Ok(())
    })();

    if let Err(error) = persistence_result {
        drop(tx);
        if managed_install {
            let _ = fs::remove_dir_all(&final_target);
        }
        if replaced_existing {
            let _ = fs::rename(&backup_target, &final_target);
        }
        return Err(error);
    }

    tx.commit()?;
    if replaced_existing {
        let _ = fs::remove_dir_all(&backup_target);
    }
    if transient_source {
        let _ = fs::remove_dir_all(&package_source_path);
    }
    sync_plugin_master_dependencies(conn, &mod_id)?;
    if preview.has_fomod {
        sync_fomod_file_dependencies(conn, &mod_id, &preview, resolved_selection.as_ref())?;
    }

    Ok(ImportedModReport {
        mod_record: ModRecord {
            id: mod_id.clone(),
            instance_id,
            name,
            version: None,
            size_bytes: files.iter().filter_map(|file| file.file_size).sum::<u64>() as i64,
            source_path,
            installed_path,
            enabled_default: true,
            tags: Vec::new(),
            notes: None,
            rule_type: None,
            rule_target_mod_id: None,
            rule_weight: 0,
        },
        files_scanned: files.len(),
        plugins_discovered: files
            .iter()
            .filter(|file| scanner::is_plugin_path(&file.original_rel_path))
            .count(),
        copied_into_workspace: preview.has_fomod || copy_into_workspace,
    })
}

fn sync_plugin_master_dependencies(conn: &mut Connection, mod_id: &str) -> SlimResult<()> {
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM mod_dependencies WHERE mod_id = ?1 AND notes LIKE '[auto:plugin-master]%'",
        params![mod_id],
    )?;

    let mut stmt = tx.prepare(
        "SELECT filename, abs_source_path
         FROM plugins p
         JOIN mod_files mf ON mf.mod_id = p.mod_id AND mf.normalized_rel_path = p.normalized_rel_path
         WHERE p.mod_id = ?1",
    )?;
    let plugin_rows = stmt.query_map(params![mod_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut inserts = Vec::new();
    for row in plugin_rows {
        let (plugin_filename, abs_source_path) = row?;
        let bytes = match fs::read(&abs_source_path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let masters = match crate::plugin_metadata::parse_plugin_masters(&bytes) {
            Ok(masters) => masters,
            Err(_) => continue,
        };
        for master in masters {
            inserts.push((plugin_filename.clone(), master));
        }
    }
    drop(stmt);

    for (plugin_filename, master) in inserts {
        tx.execute(
            "INSERT INTO mod_dependencies (
                 id, mod_id, dependency_type, target_mod_id, target_value, notes, created_at, updated_at
             )
             VALUES (?1, ?2, 'plugin', NULL, ?3, ?4, ?5, ?5)",
            params![
                Uuid::new_v4().to_string(),
                mod_id,
                master,
                format!("[auto:plugin-master] required by {plugin_filename}"),
                &now
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

fn sync_fomod_file_dependencies(
    conn: &mut Connection,
    mod_id: &str,
    preview: &FomodPackagePreview,
    selection: Option<&crate::models::FomodSelectionRequest>,
) -> SlimResult<()> {
    let mut files = BTreeSet::new();
    if let Some(group) = &preview.module_dependencies {
        collect_fomod_file_dependencies(group, &mut files);
    }
    if let Some(selection) = selection {
        for entry in &selection.selections {
            let Some(group) = preview
                .steps
                .get(entry.step_index)
                .and_then(|step| step.groups.get(entry.group_index))
            else {
                continue;
            };
            for option in &group.options {
                if entry.selected_option_ids.contains(&option.id) {
                    if let Some(dependencies) = &option.dependencies {
                        collect_fomod_file_dependencies(dependencies, &mut files);
                    }
                }
            }
        }
    }
    if files.is_empty() {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM mod_dependencies WHERE mod_id = ?1 AND notes LIKE '[auto:fomod]%'",
        params![mod_id],
    )?;
    for file in files {
        tx.execute(
            "INSERT INTO mod_dependencies (
                 id, mod_id, dependency_type, target_mod_id, target_value, notes, created_at, updated_at
             )
             VALUES (?1, ?2, 'file', NULL, ?3, ?4, ?5, ?5)",
            params![
                Uuid::new_v4().to_string(),
                mod_id,
                file,
                "[auto:fomod] fileDependency from ModuleConfig.xml",
                &now
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn collect_fomod_file_dependencies(group: &FomodDependencyGroup, out: &mut BTreeSet<String>) {
    for dependency in &group.dependencies {
        match dependency {
            FomodDependency::File { file, state, .. } => {
                if state.eq_ignore_ascii_case("active") {
                    out.insert(normalize_dependency_path(file));
                }
            }
            FomodDependency::Group(group) => collect_fomod_file_dependencies(group, out),
            FomodDependency::Flag { .. } => {}
        }
    }
}

fn validate_preview_source(workspace_root: &Path, source_path: &Path) -> SlimResult<PathBuf> {
    let preview_root = workspace_root.join("fomod-preview");
    let canonical_root = fs::canonicalize(&preview_root)?;
    let canonical_source = fs::canonicalize(source_path)?;
    if !canonical_source.starts_with(&canonical_root) {
        return Err(SlimError::Safety(format!(
            "preview source is outside the preview workspace: {}",
            source_path.display()
        )));
    }
    Ok(canonical_source)
}

pub fn discard_fomod_preview(workspace_root: &Path, preview_path: &Path) -> SlimResult<()> {
    let source = validate_preview_source(workspace_root, preview_path)?;
    let preview_root = fs::canonicalize(workspace_root.join("fomod-preview"))?;
    let removable = source
        .ancestors()
        .take_while(|candidate| *candidate != preview_root)
        .last()
        .ok_or_else(|| SlimError::Safety("invalid FOMOD preview path".into()))?;
    fs::remove_dir_all(removable)?;
    Ok(())
}

fn normalize_dependency_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches('/')
        .to_lowercase()
}

fn prepare_import_source(
    source_path: &Path,
    instance_mods_root: &Path,
    mod_id: &str,
) -> SlimResult<(PathBuf, bool)> {
    if source_path.is_dir() {
        return Ok((source_path.to_path_buf(), false));
    }
    if !source_path.exists() {
        return Err(SlimError::NotFound(format!(
            "source path does not exist: {}",
            source_path.display()
        )));
    }

    let Some(kind) = ArchiveKind::from_path(source_path) else {
        return Err(SlimError::InvalidPath(format!(
            "source path is not a supported mod folder or archive: {}",
            source_path.display()
        )));
    };

    let extraction_root = instance_mods_root.join(format!("{mod_id}-source"));
    fs::create_dir_all(&extraction_root)?;
    paths::guard_descendant(instance_mods_root, &extraction_root.join(".guard"))?;
    extract_archive(source_path, &extraction_root, kind)?;
    Ok((extraction_root, true))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    SevenZip,
    Rar,
}

impl ArchiveKind {
    fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_string_lossy().to_lowercase();
        match extension.as_str() {
            "zip" | "fomod" | "omod" => Some(Self::Zip),
            "7z" => Some(Self::SevenZip),
            "rar" => Some(Self::Rar),
            _ => None,
        }
    }
}

fn extract_archive(source_path: &Path, destination: &Path, kind: ArchiveKind) -> SlimResult<()> {
    match kind {
        ArchiveKind::Zip => extract_zip_archive(source_path, destination),
        ArchiveKind::SevenZip => extract_seven_zip_archive(source_path, destination),
        ArchiveKind::Rar => extract_rar_archive(source_path, destination),
    }
}

fn extract_zip_archive(source_path: &Path, destination: &Path) -> SlimResult<()> {
    let file = fs::File::open(source_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| SlimError::InvalidPath(format!("invalid zip archive: {error}")))?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| SlimError::InvalidPath(format!("invalid zip entry: {error}")))?;
        let Some(enclosed_name) = file.enclosed_name() else {
            return Err(SlimError::Safety(format!(
                "archive entry escapes extraction root: {}",
                file.name()
            )));
        };
        let target = safe_archive_target(destination, &enclosed_name)?;
        if file.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        std::io::copy(&mut file, &mut output)?;
    }

    Ok(())
}

fn extract_seven_zip_archive(source_path: &Path, destination: &Path) -> SlimResult<()> {
    sevenz_rust::decompress_file_with_extract_fn(source_path, destination, |entry, reader, _| {
        let target = safe_archive_target(destination, Path::new(entry.name()))
            .map_err(|error| sevenz_rust::Error::other(error.to_string()))?;
        if entry.is_directory() {
            fs::create_dir_all(&target).map_err(sevenz_rust::Error::io)?;
            return Ok(true);
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(sevenz_rust::Error::io)?;
        }
        let mut output = fs::File::create(&target).map_err(sevenz_rust::Error::io)?;
        std::io::copy(reader, &mut output).map_err(sevenz_rust::Error::io)?;
        Ok(true)
    })
    .map_err(|error| SlimError::InvalidPath(format!("invalid 7z archive: {error}")))?;
    Ok(())
}

fn extract_rar_archive(source_path: &Path, destination: &Path) -> SlimResult<()> {
    fs::create_dir_all(destination)?;
    let archive = unrar_ng::Archive::new(source_path)
        .open_for_processing()
        .map_err(|error| SlimError::InvalidPath(format!("invalid rar archive: {error}")))?;
    archive
        .extract_all(destination)
        .map_err(|error| SlimError::InvalidPath(format!("invalid rar archive: {error}")))?;
    Ok(())
}

fn safe_archive_target(destination: &Path, relative_path: &Path) -> SlimResult<PathBuf> {
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(SlimError::Safety(format!(
            "archive entry escapes extraction root: {}",
            relative_path.display()
        )));
    }

    let target = destination.join(relative_path);
    paths::guard_inside(destination, &target)?;
    Ok(target)
}

fn copy_directory_recursive(source: &Path, destination: &Path) -> SlimResult<()> {
    if !source.exists() {
        return Err(SlimError::NotFound(format!(
            "source path does not exist: {}",
            source.display()
        )));
    }

    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|err| SlimError::InvalidPath(err.to_string()))?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| SlimError::InvalidPath("failed to determine relative path".into()))?;
        let target = destination.join(relative);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &target)?;
    }

    Ok(())
}

fn loot_game_identifier(game_type: &str) -> Option<&'static str> {
    match game_type {
        "skyrimse" => Some("Skyrim Special Edition"),
        "skyrimvr" => Some("Skyrim VR"),
        "skyrim" => Some("Skyrim"),
        "fallout4" => Some("Fallout4"),
        "fallout4vr" => Some("Fallout4VR"),
        "falloutnv" => Some("FalloutNV"),
        "fallout3" => Some("Fallout3"),
        "oblivion" => Some("Oblivion"),
        "morrowind" => Some("Morrowind"),
        "enderal" => Some("Enderal"),
        "enderalse" => Some("Enderal Special Edition"),
        "nehrim" => Some("Nehrim"),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct LootLaunchContext {
    game_local_path: PathBuf,
    args: Vec<String>,
}

fn prepare_loot_launch_context(
    workspace_root: &Path,
    instance_id: &str,
    game_identifier: &str,
    install_path: &Path,
) -> SlimResult<LootLaunchContext> {
    let loot_data_path = paths::loot_data_path(workspace_root, instance_id);
    prepare_loot_launch_context_with_data_root(&loot_data_path, game_identifier, install_path)
}

fn prepare_loot_launch_context_with_data_root(
    loot_data_path: &Path,
    game_identifier: &str,
    install_path: &Path,
) -> SlimResult<LootLaunchContext> {
    let game_local_path = loot_data_path
        .join("local-appdata")
        .join(sanitise_loot_local_folder_name(game_identifier));

    fs::create_dir_all(loot_data_path)?;
    fs::create_dir_all(&game_local_path)?;
    write_loot_settings(
        loot_data_path,
        game_identifier,
        install_path,
        &game_local_path,
    )?;

    let args = vec![
        format!("--game={game_identifier}"),
        format!("--game-path={}", install_path.display()),
        format!("--loot-data-path={}", loot_data_path.display()),
        "--auto-sort".to_string(),
    ];

    Ok(LootLaunchContext {
        game_local_path,
        args,
    })
}

fn seed_loot_local_files(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
    game_local_path: &Path,
) -> SlimResult<()> {
    fs::create_dir_all(game_local_path)?;
    let plugins = profile_plugin_filenames(conn, instance_id, profile_id, true)?;
    let loadorder = profile_plugin_filenames(conn, instance_id, profile_id, false)?;
    fs::write(game_local_path.join("plugins.txt"), plugins.join("\n"))?;
    fs::write(game_local_path.join("loadorder.txt"), loadorder.join("\n"))?;
    Ok(())
}

fn profile_plugin_filenames(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
    _active_only: bool,
) -> SlimResult<Vec<String>> {
    ensure_plugin_index_for_instance(conn, instance_id)?;

    let mut stmt = conn.prepare(
        "SELECT p.filename
         FROM plugins p
         JOIN mods m ON m.id = p.mod_id
         LEFT JOIN profile_mods pm ON pm.mod_id = m.id AND pm.profile_id = ?2
         LEFT JOIN profile_plugins pp ON pp.plugin_id = p.id AND pp.profile_id = ?2
         WHERE m.instance_id = ?1
           AND COALESCE(pm.enabled, m.enabled_default) = 1
           AND COALESCE(pp.enabled, 1) = 1
         ORDER BY COALESCE(pm.priority, 0) ASC, LOWER(p.filename) ASC",
    )?;
    let rows = stmt.query_map(params![instance_id, profile_id], |row| {
        row.get::<_, String>(0)
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub(crate) fn ensure_plugin_index_for_instance(
    conn: &Connection,
    instance_id: &str,
) -> SlimResult<()> {
    let mut stmt = conn.prepare(
        "SELECT mf.mod_id, mf.original_rel_path, mf.normalized_rel_path
         FROM mod_files mf
         JOIN mods m ON m.id = mf.mod_id
         LEFT JOIN plugins p ON p.mod_id = mf.mod_id
             AND p.normalized_rel_path = mf.normalized_rel_path
         WHERE m.instance_id = ?1
           AND p.id IS NULL",
    )?;
    let rows = stmt.query_map(params![instance_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut missing_plugins = Vec::new();
    for row in rows {
        let (mod_id, original_rel_path, normalized_rel_path) = row?;
        if !scanner::is_plugin_path(&original_rel_path) {
            continue;
        }
        let filename = PathBuf::from(&original_rel_path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(&original_rel_path)
            .to_string();
        let plugin_type = Path::new(&filename)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        missing_plugins.push((mod_id, filename, plugin_type, normalized_rel_path));
    }
    drop(stmt);

    if missing_plugins.is_empty() {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    for (mod_id, filename, plugin_type, normalized_rel_path) in missing_plugins {
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                mod_id,
                filename,
                plugin_type,
                normalized_rel_path,
                &now
            ],
        )?;
    }

    Ok(())
}

fn write_loot_settings(
    loot_data_path: &Path,
    game_identifier: &str,
    install_path: &Path,
    game_local_path: &Path,
) -> SlimResult<()> {
    let settings_path = loot_data_path.join("settings.toml");
    let content = format!(
        "default_game = \"{}\"\n\n[[games]]\ngameId = \"{}\"\nfolder = \"{}\"\npath = \"{}\"\nlocal_path = \"{}\"\n",
        toml_escape(game_identifier),
        toml_escape(game_identifier),
        toml_escape(game_identifier),
        toml_escape(&install_path.to_string_lossy()),
        toml_escape(&game_local_path.to_string_lossy()),
    );

    fs::write(settings_path, content)?;
    Ok(())
}

fn sanitise_loot_local_folder_name(game_identifier: &str) -> String {
    game_identifier
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => character,
        })
        .collect()
}

fn toml_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            _ => vec![character],
        })
        .collect()
}

fn import_loot_order(
    conn: &mut Connection,
    instance_id: &str,
    profile_id: &str,
    game_local_path: &Path,
) -> SlimResult<usize> {
    let loadorder_file = find_loot_file(game_local_path, "loadorder.txt");
    let plugins_file = find_loot_file(game_local_path, "plugins.txt");
    let order_file = loadorder_file
        .or_else(|| plugins_file.clone())
        .ok_or_else(|| SlimError::NotFound("LOOT did not produce a load order file".into()))?;
    let active_file = plugins_file.unwrap_or_else(|| order_file.clone());

    let ordered_files = read_loot_order_lines(&order_file)?;
    let active_files = read_loot_active_lines(&active_file)?;

    let plugin_to_mod = load_profile_enabled_plugin_mod_map(conn, instance_id, profile_id)?;
    let mut mod_positions: BTreeMap<String, usize> = BTreeMap::new();
    let mut mod_active: BTreeSet<String> = BTreeSet::new();

    for (index, filename) in ordered_files.iter().enumerate() {
        let key = filename.to_lowercase();
        if let Some(mod_id) = plugin_to_mod.get(&key) {
            mod_positions
                .entry(mod_id.clone())
                .and_modify(|position| *position = (*position).max(index))
                .or_insert(index);
        }
    }

    for filename in active_files {
        if let Some(mod_id) = plugin_to_mod.get(&filename.to_lowercase()) {
            mod_active.insert(mod_id.clone());
        }
    }

    let current_rows = load_profile_mod_state(conn, instance_id, profile_id)?;
    let mut ordered_mods: Vec<(String, usize)> = mod_positions.into_iter().collect();
    ordered_mods.sort_by_key(|(_, position)| *position);

    let mut updates = Vec::new();
    let mut next_priority = 0;
    for (mod_id, _) in ordered_mods {
        let enabled = mod_active.contains(&mod_id)
            || current_rows
                .get(&mod_id)
                .map(|entry| entry.enabled)
                .unwrap_or(true);
        updates.push((mod_id, enabled, next_priority));
        next_priority += 1;
    }

    for (mod_id, entry) in current_rows {
        if updates
            .iter()
            .any(|(updated_id, _, _)| updated_id == &mod_id)
        {
            continue;
        }
        updates.push((mod_id, entry.enabled, entry.priority));
    }

    let tx = conn.transaction()?;
    for (mod_id, enabled, priority) in updates {
        tx.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(profile_id, mod_id) DO UPDATE SET
                 enabled = excluded.enabled,
                 priority = excluded.priority",
            params![profile_id, &mod_id, if enabled { 1 } else { 0 }, priority],
        )?;
    }
    tx.commit()?;

    Ok(ordered_files.len())
}

fn mod_instance_id(conn: &Connection, mod_id: &str) -> SlimResult<String> {
    conn.query_row(
        "SELECT instance_id FROM mods WHERE id = ?1",
        params![mod_id],
        |row| row.get(0),
    )
    .map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => {
            SlimError::NotFound(format!("mod not found: {mod_id}"))
        }
        other => other.into(),
    })
}

fn profile_instance_id(conn: &Connection, profile_id: &str) -> SlimResult<String> {
    conn.query_row(
        "SELECT instance_id FROM profiles WHERE id = ?1",
        params![profile_id],
        |row| row.get(0),
    )
    .map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => {
            SlimError::NotFound(format!("profile not found: {profile_id}"))
        }
        other => other.into(),
    })
}

fn plugin_instance_id(conn: &Connection, plugin_id: &str) -> SlimResult<String> {
    conn.query_row(
        "SELECT m.instance_id
         FROM plugins p
         JOIN mods m ON m.id = p.mod_id
         WHERE p.id = ?1",
        params![plugin_id],
        |row| row.get(0),
    )
    .map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => {
            SlimError::NotFound(format!("plugin not found: {plugin_id}"))
        }
        other => other.into(),
    })
}

fn normalize_dependency_type(value: &str) -> SlimResult<&'static str> {
    match value.trim().to_lowercase().as_str() {
        "mod" => Ok("mod"),
        "plugin" => Ok("plugin"),
        "file" => Ok("file"),
        "manual" => Ok("manual"),
        other => Err(SlimError::InvalidPath(format!(
            "unsupported dependency type: {other}"
        ))),
    }
}

fn dependency_satisfied_for_profile(
    conn: &Connection,
    instance_id: &str,
    profile_id: Option<&str>,
    dependency_type: &str,
    target_mod_id: Option<&str>,
    target_value: &str,
) -> SlimResult<bool> {
    match dependency_type {
        "mod" => {
            dependency_mod_satisfied(conn, instance_id, profile_id, target_mod_id, target_value)
        }
        "plugin" => dependency_plugin_satisfied(conn, instance_id, profile_id, target_value),
        "file" => dependency_file_satisfied(conn, instance_id, profile_id, target_value),
        "manual" => Ok(false),
        _ => Ok(false),
    }
}

fn dependency_mod_satisfied(
    conn: &Connection,
    instance_id: &str,
    profile_id: Option<&str>,
    target_mod_id: Option<&str>,
    target_value: &str,
) -> SlimResult<bool> {
    if let Some(profile_id) = profile_id {
        let count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM mods m
             LEFT JOIN profile_mods pm ON pm.mod_id=m.id AND pm.profile_id=?3
             WHERE m.instance_id=?1 AND COALESCE(pm.enabled,m.enabled_default)=1
             AND ((?2 IS NOT NULL AND m.id=?2) OR (?2 IS NULL AND LOWER(m.name)=LOWER(?4)))",
            params![instance_id, target_mod_id, profile_id, target_value.trim()],
            |row| row.get(0),
        )?;
        return Ok(count > 0);
    }
    if let Some(target_mod_id) = target_mod_id {
        let count: i64 = conn.query_row(
            "SELECT COUNT(1) FROM mods WHERE id = ?1 AND instance_id = ?2",
            params![target_mod_id, instance_id],
            |row| row.get(0),
        )?;
        return Ok(count > 0);
    }

    let count: i64 = conn.query_row(
        "SELECT COUNT(1) FROM mods WHERE instance_id = ?1 AND LOWER(name) = LOWER(?2)",
        params![instance_id, target_value.trim()],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn dependency_plugin_satisfied(
    conn: &Connection,
    instance_id: &str,
    profile_id: Option<&str>,
    target_value: &str,
) -> SlimResult<bool> {
    let normalized_target = normalize_plugin_dependency_target(target_value);
    if normalized_target.is_empty() {
        return Ok(false);
    }

    let count: i64 = if let Some(profile_id) = profile_id {
        conn.query_row(
            "SELECT COUNT(1) FROM plugins p JOIN mods m ON m.id=p.mod_id
             LEFT JOIN profile_mods pm ON pm.mod_id=m.id AND pm.profile_id=?3
             LEFT JOIN profile_plugins pp ON pp.plugin_id=p.id AND pp.profile_id=?3
             WHERE m.instance_id=?1 AND LOWER(p.filename)=LOWER(?2)
             AND COALESCE(pm.enabled,m.enabled_default)=1 AND COALESCE(pp.enabled,1)=1",
            params![instance_id, &normalized_target, profile_id],
            |row| row.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(1) FROM plugins p JOIN mods m ON m.id=p.mod_id
             WHERE m.instance_id=?1 AND LOWER(p.filename)=LOWER(?2)",
            params![instance_id, &normalized_target],
            |row| row.get(0),
        )?
    };

    if count > 0 {
        return Ok(true);
    }

    let instance = crate::instance::get_instance_by_id(conn, instance_id)?;
    if file_exists_case_insensitive(&instance.data_path, &normalized_target) {
        return Ok(true);
    }

    if is_standard_game_master(&instance.game_type, &normalized_target) {
        return Ok(true);
    }

    Ok(false)
}

fn normalize_plugin_dependency_target(target_value: &str) -> String {
    let trimmed = target_value
        .trim()
        .trim_start_matches('*')
        .replace('\\', "/");
    let without_data = trimmed
        .strip_prefix("Data/")
        .or_else(|| trimmed.strip_prefix("data/"))
        .unwrap_or(&trimmed);
    Path::new(without_data)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(without_data)
        .trim()
        .to_string()
}

fn is_standard_game_master(game_type: &str, filename: &str) -> bool {
    let filename = filename.trim().to_ascii_lowercase();
    if !matches!(
        game_type.trim().to_ascii_lowercase().as_str(),
        "skyrim" | "skyrimse" | "skyrimvr"
    ) {
        return false;
    }
    matches!(
        filename.as_str(),
        "skyrim.esm" | "update.esm" | "dawnguard.esm" | "hearthfires.esm" | "dragonborn.esm"
    )
}

fn file_exists_case_insensitive(root: &Path, filename: &str) -> bool {
    if filename.trim().is_empty() {
        return false;
    }

    let target_lower = filename.to_lowercase();
    if root.join(filename).exists() {
        return true;
    }

    fs::read_dir(root).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry.file_name().to_string_lossy().to_lowercase() == target_lower
        })
    })
}

fn dependency_file_satisfied(
    conn: &Connection,
    instance_id: &str,
    profile_id: Option<&str>,
    target_value: &str,
) -> SlimResult<bool> {
    let dependency_context =
        build_fomod_dependency_context_for_profile(conn, instance_id, profile_id)?;
    Ok(fomod::file_dependency_is_satisfied(
        target_value,
        "Active",
        Some(&dependency_context),
    ))
}

pub fn build_fomod_dependency_context(
    conn: &Connection,
    instance_id: &str,
) -> SlimResult<FomodDependencyContext> {
    build_fomod_dependency_context_for_profile(conn, instance_id, None)
}

pub fn build_fomod_dependency_context_for_profile(
    conn: &Connection,
    instance_id: &str,
    profile_id: Option<&str>,
) -> SlimResult<FomodDependencyContext> {
    let mut available_files = BTreeSet::new();

    let mut stmt = conn.prepare(
        "SELECT mf.normalized_rel_path
         FROM mod_files mf
         JOIN mods m ON m.id = mf.mod_id
         LEFT JOIN profile_mods pm ON pm.mod_id=m.id AND pm.profile_id=?2
         WHERE m.instance_id = ?1
           AND (?2 IS NULL OR COALESCE(pm.enabled,m.enabled_default)=1)",
    )?;
    let rows = stmt.query_map(params![instance_id, profile_id], |row| {
        row.get::<_, String>(0)
    })?;
    for row in rows {
        available_files.insert(row?);
    }

    let instance = crate::instance::get_instance_by_id(conn, instance_id)?;
    let mut search_roots = Vec::new();
    search_roots.push(instance.data_path);

    Ok(FomodDependencyContext::from_roots(
        available_files,
        search_roots,
    ))
}

#[derive(Debug, Clone)]
struct ProfileModState {
    enabled: bool,
    priority: i64,
}

fn load_profile_mod_state(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<std::collections::BTreeMap<String, ProfileModState>> {
    let mut stmt = conn.prepare(
        "SELECT pm.mod_id, pm.enabled, pm.priority
         FROM profile_mods pm
         JOIN mods m ON m.id = pm.mod_id
         WHERE m.instance_id = ?1
           AND pm.profile_id = ?2",
    )?;

    let rows = stmt.query_map(params![instance_id, profile_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            ProfileModState {
                enabled: row.get::<_, i64>(1)? != 0,
                priority: row.get(2)?,
            },
        ))
    })?;

    let mut out = BTreeMap::new();
    for row in rows {
        let (mod_id, state) = row?;
        out.insert(mod_id, state);
    }
    Ok(out)
}

fn load_profile_enabled_plugin_mod_map(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<BTreeMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT p.filename, p.mod_id
         FROM plugins p
         JOIN mods m ON m.id = p.mod_id
         LEFT JOIN profile_plugins pp ON pp.plugin_id = p.id AND pp.profile_id = ?2
         WHERE m.instance_id = ?1
           AND COALESCE(pp.enabled, 1) = 1",
    )?;

    let rows = stmt.query_map(params![instance_id, profile_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut out = BTreeMap::new();
    for row in rows {
        let (filename, mod_id) = row?;
        out.insert(filename.to_lowercase(), mod_id);
    }
    Ok(out)
}

fn find_loot_file(root: &Path, target_name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(target_name)
        {
            candidates.push(entry.path().to_path_buf());
        }
    }
    candidates.sort();
    candidates.pop()
}

fn read_loot_order_lines(path: &Path) -> SlimResult<Vec<String>> {
    let content = fs::read_to_string(path)?;
    Ok(content
        .lines()
        .map(|line| line.trim().trim_start_matches('*').trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect())
}

fn read_loot_active_lines(path: &Path) -> SlimResult<BTreeSet<String>> {
    let content = fs::read_to_string(path)?;
    let mut active = BTreeSet::new();
    let mut plain = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('*') {
            active.insert(trimmed.trim_start_matches('*').trim().to_lowercase());
        } else {
            plain.insert(trimmed.to_lowercase());
        }
    }
    if active.is_empty() {
        Ok(plain)
    } else {
        Ok(active)
    }
}

fn preview_loot_order_from_files(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
    game_local_path: &Path,
) -> SlimResult<Vec<ProfileModEntry>> {
    let loadorder_file = find_loot_file(game_local_path, "loadorder.txt");
    let plugins_file = find_loot_file(game_local_path, "plugins.txt");
    let order_file = loadorder_file
        .or_else(|| plugins_file.clone())
        .ok_or_else(|| SlimError::NotFound("LOOT did not produce a load order file".into()))?;
    let active_file = plugins_file.unwrap_or_else(|| order_file.clone());

    let ordered_files = read_loot_order_lines(&order_file)?;
    let active_files = read_loot_active_lines(&active_file)?;
    let plugin_to_mod = load_profile_enabled_plugin_mod_map(conn, instance_id, profile_id)?;
    let current_entries = list_profile_mods(conn, instance_id, profile_id)?;
    let mut entry_map: BTreeMap<String, ProfileModEntry> = current_entries
        .iter()
        .cloned()
        .map(|entry| (entry.mod_id.clone(), entry))
        .collect();
    let mut mod_positions: BTreeMap<String, usize> = BTreeMap::new();

    for (index, filename) in ordered_files.iter().enumerate() {
        if let Some(mod_id) = plugin_to_mod.get(&filename.to_lowercase()) {
            mod_positions
                .entry(mod_id.clone())
                .and_modify(|position| *position = (*position).max(index))
                .or_insert(index);
        }
    }

    let mut ordered_mods: Vec<(String, usize)> = mod_positions.into_iter().collect();
    ordered_mods.sort_by_key(|(_, position)| *position);

    let mut preview = Vec::new();
    for (next_priority, (mod_id, _)) in ordered_mods.into_iter().enumerate() {
        if let Some(mut entry) = entry_map.remove(&mod_id) {
            let plugin_names = mod_plugin_filenames(conn, &mod_id)?;
            entry.enabled = plugin_names
                .iter()
                .any(|filename| active_files.contains(&filename.to_lowercase()));
            entry.priority = next_priority as i64;
            preview.push(entry);
        }
    }

    let mut remaining: Vec<_> = entry_map.into_values().collect();
    remaining.sort_by(|left, right| {
        right.priority.cmp(&left.priority).then_with(|| {
            left.mod_name
                .to_lowercase()
                .cmp(&right.mod_name.to_lowercase())
        })
    });
    preview.extend(remaining);

    Ok(preview)
}

fn mod_plugin_filenames(conn: &Connection, mod_id: &str) -> SlimResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT filename
         FROM plugins
         WHERE mod_id = ?1
         ORDER BY LOWER(filename)",
    )?;
    let rows = stmt.query_map(params![mod_id], |row| row.get::<_, String>(0))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_workspace(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("slimcc-{name}-{}", Uuid::new_v4()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn profile_plugin_state_round_trips_disabled_plugin_for_profile() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(
            "CREATE TABLE profile_plugins (
                profile_id TEXT NOT NULL,
                plugin_id TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(profile_id, plugin_id),
                FOREIGN KEY(profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
                FOREIGN KEY(plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
            );",
        )
        .expect("create profile plugin schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-a', 'instance-a', 'Default', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert profile");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES ('plugin-a', 'mod-a', 'Patch.esp', 'esp', 'patch.esp', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert plugin");

        let initial = list_profile_plugins(&conn, "instance-a", "profile-a")
            .expect("list initial profile plugins");
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].plugin_id, "plugin-a");
        assert!(initial[0].enabled);
        assert!(initial[0].mod_enabled);

        let updated = update_profile_plugins(
            &mut conn,
            crate::models::UpdateProfilePluginsRequest {
                profile_id: "profile-a".into(),
                entries: vec![crate::models::UpdateProfilePluginEntry {
                    plugin_id: "plugin-a".into(),
                    enabled: false,
                }],
            },
        )
        .expect("update profile plugins");

        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].plugin_id, "plugin-a");
        assert!(!updated[0].enabled);

        let listed = list_profile_plugins(&conn, "instance-a", "profile-a")
            .expect("list updated profile plugins");
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].enabled);
    }

    #[test]
    fn profile_plugins_repairs_missing_esm_plugin_index_rows() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-a', 'instance-a', 'Default', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert profile");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'mod-a', 1, 0)",
            [],
        )
        .expect("insert profile mod");
        conn.execute(
            "INSERT INTO mod_files (id, mod_id, original_rel_path, normalized_rel_path, abs_source_path, created_at)
             VALUES ('file-main', 'mod-a', 'Data/Main.esm', 'main.esm', '/mods/a/Data/Main.esm', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod file");

        let listed = list_profile_plugins(&conn, "instance-a", "profile-a")
            .expect("list repaired profile plugins");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].filename, "Main.esm");
        assert_eq!(listed[0].plugin_type, "esm");
    }

    #[test]
    fn profile_plugin_filenames_skip_disabled_profile_plugins() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(
            "CREATE TABLE profile_plugins (
                profile_id TEXT NOT NULL,
                plugin_id TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(profile_id, plugin_id),
                FOREIGN KEY(profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
                FOREIGN KEY(plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
            );",
        )
        .expect("create profile plugin schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-a', 'instance-a', 'Default', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert profile");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'mod-a', 1, 0)",
            [],
        )
        .expect("insert profile mod");
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES ('plugin-main', 'mod-a', 'Main.esm', 'esm', 'main.esm', '2026-01-01T00:00:00Z'),
                    ('plugin-patch', 'mod-a', 'Patch.esp', 'esp', 'patch.esp', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert plugins");
        conn.execute(
            "INSERT INTO profile_plugins (profile_id, plugin_id, enabled, updated_at)
             VALUES ('profile-a', 'plugin-patch', 0, '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("disable plugin");

        let active = profile_plugin_filenames(&conn, "instance-a", "profile-a", true)
            .expect("active plugin filenames");
        let loadorder = profile_plugin_filenames(&conn, "instance-a", "profile-a", false)
            .expect("loadorder plugin filenames");

        assert_eq!(active, vec!["Main.esm"]);
        assert_eq!(loadorder, vec!["Main.esm"]);
    }

    #[test]
    fn seed_loot_local_files_writes_current_profile_plugins_for_no_change_loot_runs() {
        let workspace_root = temp_workspace("loot-seed-files");
        let local_path = workspace_root.join("loot-local");
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-a', 'instance-a', 'Default', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert profile");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'mod-a', 1, 0)",
            [],
        )
        .expect("insert profile mod");
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES ('plugin-a', 'mod-a', 'Main.esm', 'esm', 'main.esm', '2026-01-01T00:00:00Z'),
                    ('plugin-b', 'mod-a', 'Patch.esp', 'esp', 'patch.esp', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert plugins");
        conn.execute(
            "INSERT INTO profile_plugins (profile_id, plugin_id, enabled, updated_at)
             VALUES ('profile-a', 'plugin-b', 0, '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("disable plugin");

        seed_loot_local_files(&conn, "instance-a", "profile-a", &local_path)
            .expect("seed loot local files");

        assert_eq!(
            fs::read_to_string(local_path.join("plugins.txt")).expect("read plugins"),
            "Main.esm"
        );
        assert_eq!(
            fs::read_to_string(local_path.join("loadorder.txt")).expect("read loadorder"),
            "Main.esm"
        );

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn loot_launch_context_uses_settings_local_path_not_removed_cli_arg() {
        let workspace_root = temp_workspace("loot-local-path");
        let install_path = workspace_root.join("steamapps/common/Skyrim Special Edition");
        fs::create_dir_all(&install_path).unwrap();

        let context = prepare_loot_launch_context(
            &workspace_root,
            "instance-a",
            "Skyrim Special Edition",
            &install_path,
        )
        .unwrap();

        assert!(context
            .args
            .iter()
            .all(|arg| !arg.starts_with("--game-appdata-path")));
        assert!(context
            .args
            .contains(&"--game=Skyrim Special Edition".to_string()));
        assert!(context.args.contains(&format!(
            "--loot-data-path={}",
            paths::loot_data_path(&workspace_root, "instance-a").display()
        )));

        let loot_data_path = paths::loot_data_path(&workspace_root, "instance-a");
        let expected_local_path = loot_data_path
            .join("local-appdata")
            .join("Skyrim Special Edition");
        assert_eq!(expected_local_path, context.game_local_path);
        assert!(context.game_local_path.is_dir());

        let settings = fs::read_to_string(loot_data_path.join("settings.toml")).unwrap();
        assert!(settings.contains("default_game = \"Skyrim Special Edition\""));
        assert!(settings.contains("gameId = \"Skyrim Special Edition\""));
        assert!(settings.contains("local_path = "));

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn loot_import_assigns_zero_based_order_and_treats_plain_plugins_as_active() {
        let workspace_root = temp_workspace("loot-import-order");
        let local_path = workspace_root.join("loot-local");
        fs::create_dir_all(&local_path).unwrap();
        fs::write(local_path.join("loadorder.txt"), "ModB.esp\nModA.esp\n").unwrap();
        fs::write(local_path.join("plugins.txt"), "ModB.esp\nModA.esp\n").unwrap();

        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-a', 'instance-a', 'Default', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert profile");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('mod-b', 'instance-a', 'Mod B', '/mods/b', '/mods/b', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mods");
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES ('plugin-a', 'mod-a', 'ModA.esp', 'esp', 'moda.esp', '2026-01-01T00:00:00Z'),
                    ('plugin-b', 'mod-b', 'ModB.esp', 'esp', 'modb.esp', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert plugins");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'mod-a', 0, 50),
                    ('profile-a', 'mod-b', 0, 40)",
            [],
        )
        .expect("insert profile mods");

        let synced = import_loot_order(&mut conn, "instance-a", "profile-a", &local_path).unwrap();
        let profile_mods = list_profile_mods(&conn, "instance-a", "profile-a").unwrap();
        let mod_a = profile_mods
            .iter()
            .find(|entry| entry.mod_id == "mod-a")
            .unwrap();
        let mod_b = profile_mods
            .iter()
            .find(|entry| entry.mod_id == "mod-b")
            .unwrap();

        assert_eq!(synced, 2);
        assert!(mod_a.enabled);
        assert!(mod_b.enabled);
        assert_eq!(mod_b.priority, 0);
        assert_eq!(mod_a.priority, 1);

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn loot_import_ignores_disabled_profile_plugins_when_syncing_active_mods() {
        let workspace_root = temp_workspace("loot-import-disabled-plugin");
        let local_path = workspace_root.join("loot-local");
        fs::create_dir_all(&local_path).unwrap();
        fs::write(local_path.join("loadorder.txt"), "Patch.esp\n").unwrap();
        fs::write(local_path.join("plugins.txt"), "Patch.esp\n").unwrap();

        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-a', 'instance-a', 'Default', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert profile");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Patch Mod', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES ('plugin-patch', 'mod-a', 'Patch.esp', 'esp', 'patch.esp', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert plugin");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'mod-a', 0, 50)",
            [],
        )
        .expect("insert profile mod");
        conn.execute(
            "INSERT INTO profile_plugins (profile_id, plugin_id, enabled, updated_at)
             VALUES ('profile-a', 'plugin-patch', 0, '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("disable plugin");

        let synced = import_loot_order(&mut conn, "instance-a", "profile-a", &local_path).unwrap();
        let profile_mods = list_profile_mods(&conn, "instance-a", "profile-a").unwrap();
        let mod_a = profile_mods
            .iter()
            .find(|entry| entry.mod_id == "mod-a")
            .unwrap();

        assert_eq!(synced, 1);
        assert!(!mod_a.enabled);

        let plugin_enabled: i64 = conn
            .query_row(
                "SELECT enabled FROM profile_plugins WHERE profile_id = 'profile-a' AND plugin_id = 'plugin-patch'",
                [],
                |row| row.get(0),
            )
            .expect("read plugin state");
        assert_eq!(plugin_enabled, 0);

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn loot_preview_uses_loot_output_without_mutating_profile_order() {
        let workspace_root = temp_workspace("loot-preview-order");
        let local_path = workspace_root.join("loot-local");
        fs::create_dir_all(&local_path).unwrap();
        fs::write(local_path.join("loadorder.txt"), "ModB.esp\nModA.esp\n").unwrap();
        fs::write(local_path.join("plugins.txt"), "ModB.esp\nModA.esp\n").unwrap();

        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-a', 'instance-a', 'Default', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert profile");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('mod-b', 'instance-a', 'Mod B', '/mods/b', '/mods/b', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mods");
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES ('plugin-a', 'mod-a', 'ModA.esp', 'esp', 'moda.esp', '2026-01-01T00:00:00Z'),
                    ('plugin-b', 'mod-b', 'ModB.esp', 'esp', 'modb.esp', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert plugins");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'mod-a', 1, 50),
                    ('profile-a', 'mod-b', 1, 40)",
            [],
        )
        .expect("insert profile mods");

        let current_before = list_profile_mods(&conn, "instance-a", "profile-a").unwrap();
        let proposed = preview_loot_order_from_files(&conn, "instance-a", "profile-a", &local_path)
            .expect("preview order");
        let current_after = list_profile_mods(&conn, "instance-a", "profile-a").unwrap();

        assert_eq!(
            proposed
                .iter()
                .map(|entry| entry.mod_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mod-b", "mod-a"]
        );
        assert_eq!(
            current_before
                .iter()
                .map(|entry| (entry.mod_id.as_str(), entry.priority))
                .collect::<Vec<_>>(),
            current_after
                .iter()
                .map(|entry| (entry.mod_id.as_str(), entry.priority))
                .collect::<Vec<_>>()
        );

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn loot_tool_request_uses_tool_profile_runner_and_effective_defaults() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(
            "CREATE TABLE app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .expect("create settings schema");
        crate::tools::initialize_tool_profile_schema(&conn).expect("create tool schema");
        conn.execute(
            "INSERT INTO app_settings (key, value)
             VALUES ('default_install_path', '/games/Skyrim Special Edition'),
                    ('default_data_path', '/games/Skyrim Special Edition/Data'),
                    ('default_wine_prefix', '/wine/skyrim')",
            [],
        )
        .expect("insert settings");
        crate::tools::upsert_tool_profile(
            &mut conn,
            crate::tools::UpdateToolProfileRequest {
                tool_key: "loot".into(),
                display_name: "LOOT".into(),
                executable_path: Some(PathBuf::from("/tools/LOOT/LOOT.exe")),
                runner_type: "Wine".into(),
                arguments: Vec::new(),
                working_directory: None,
                wine_prefix: None,
                log_path: None,
                enabled: true,
            },
        )
        .expect("save loot profile");

        let request = build_loot_tool_launch_request(
            &conn,
            Path::new("/settings/loot"),
            &["--game=Skyrim Special Edition".into()],
            Path::new("/instance/skyrim"),
            Some(Path::new("/instance/prefix")),
        )
        .expect("build loot request");
        let preview = tools::build_command_preview(&request).expect("preview");

        assert_eq!(preview.program, "wine");
        assert_eq!(preview.args[0], "/tools/LOOT/LOOT.exe");
        assert_eq!(preview.args[1], "--game=Skyrim Special Edition");
        assert_eq!(
            request.working_directory,
            Some(PathBuf::from("/instance/skyrim"))
        );
        assert_eq!(request.wine_prefix, Some(PathBuf::from("/wine/skyrim")));
    }

    #[test]
    fn import_mod_folder_extracts_zip_archive_into_workspace() {
        let workspace_root = temp_workspace("zip-import");
        let archive_path = workspace_root.join("downloads").join("SkyUI.zip");
        fs::create_dir_all(archive_path.parent().unwrap()).unwrap();
        write_test_zip(
            &archive_path,
            &[
                ("SkyUI.esp", b"plugin".as_slice()),
                ("meshes/readme.txt", b"mesh"),
            ],
        );

        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");

        let report = import_mod_folder(
            &mut conn,
            &workspace_root,
            ImportModFolderRequest {
                instance_id: "instance-a".into(),
                profile_id: None,
                target_mod_id: None,
                preview_source_path: None,
                name: "SkyUI".into(),
                source_path: archive_path.clone(),
                copy_into_workspace: true,
                fomod_selection: None,
            },
        )
        .expect("import archive");

        assert!(report.copied_into_workspace);
        assert_eq!(report.plugins_discovered, 1);
        assert!(report.mod_record.installed_path.join("SkyUI.esp").is_file());
        assert!(report
            .mod_record
            .installed_path
            .starts_with(paths::mods_root(&workspace_root, "instance-a")));

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn import_fomod_archive_preserves_download_source_and_preview_state() {
        let workspace_root = temp_workspace("fomod-import-source");
        let archive_path = workspace_root.join("downloads").join("Installer.zip");
        fs::create_dir_all(archive_path.parent().unwrap()).unwrap();
        let config_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<fomod>
  <moduleName>Archive Installer</moduleName>
  <installSteps>
    <installStep name="Options">
      <optionalFileGroups>
        <group name="Main" type="SelectExactlyOne">
          <plugins>
            <plugin name="Default">
              <files><folder source="Data" destination="" /></files>
            </plugin>
          </plugins>
        </group>
      </optionalFileGroups>
    </installStep>
  </installSteps>
</fomod>
"#;
        write_test_zip(
            &archive_path,
            &[
                ("fomod/ModuleConfig.xml", config_xml.as_bytes()),
                ("Data/example.esp", b"plugin".as_slice()),
            ],
        );

        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");

        let report = import_mod_folder(
            &mut conn,
            &workspace_root,
            ImportModFolderRequest {
                instance_id: "instance-a".into(),
                profile_id: None,
                target_mod_id: None,
                preview_source_path: None,
                name: "Archive Installer".into(),
                source_path: archive_path.clone(),
                copy_into_workspace: true,
                fomod_selection: None,
            },
        )
        .expect("import fomod archive");

        assert_eq!(report.mod_record.source_path, archive_path);

        let preview = preview_mod_fomod(&conn, &report.mod_record.id).expect("preview fomod");
        assert!(preview.has_fomod);

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn preview_fomod_package_extracts_archive_before_inspection() {
        let workspace_root = temp_workspace("fomod-preview-archive");
        let archive_path = workspace_root.join("downloads").join("Installer.zip");
        fs::create_dir_all(archive_path.parent().unwrap()).unwrap();
        let config_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<fomod>
  <moduleName>Archive Installer</moduleName>
  <installSteps>
    <installStep name="Options">
      <optionalFileGroups>
        <group name="Main" type="SelectExactlyOne">
          <plugins>
            <plugin name="Default">
              <files><folder source="Data" destination="" /></files>
            </plugin>
          </plugins>
        </group>
      </optionalFileGroups>
    </installStep>
  </installSteps>
</fomod>
"#;
        write_test_zip(
            &archive_path,
            &[
                ("fomod/ModuleConfig.xml", config_xml.as_bytes()),
                ("Data/example.esp", b"plugin".as_slice()),
            ],
        );

        let preview =
            preview_fomod_package_with_context(&workspace_root, &archive_path, None, None)
                .expect("preview archive fomod");

        assert!(preview.has_fomod);
        assert!(preview.source_path.is_dir());
        assert_ne!(preview.source_path, archive_path);
        assert!(preview.source_path.join("Data/example.esp").is_file());

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn preview_real_fomod_fixture_when_available() {
        let fixture_path = std::env::var("SLIMCC_FOMOD_FIXTURE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("test-fixtures")
                    .join("Example-Fomod.7z")
            });
        if !fixture_path.is_file() {
            eprintln!(
                "skipping real FOMOD fixture test; fixture not found: {}",
                fixture_path.display()
            );
            return;
        }

        let workspace_root = temp_workspace("real-fomod-fixture");
        let preview =
            preview_fomod_package_with_context(&workspace_root, &fixture_path, None, None)
                .expect("preview real FOMOD fixture");

        assert!(preview.has_fomod);
        assert!(preview.source_path.is_dir());
        assert!(!preview.steps.is_empty());
        assert!(preview
            .steps
            .iter()
            .flat_map(|step| step.groups.iter())
            .flat_map(|group| group.options.iter())
            .any(|option| !option.name.trim().is_empty()));

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn mod_dependencies_report_satisfied_mod_and_missing_plugin_requirements() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('dependent', 'instance-a', 'Dependent Mod', '/mods/dependent', '/mods/dependent', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('required', 'instance-a', 'Required Mod', '/mods/required', '/mods/required', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mods");

        update_mod_dependencies(
            &mut conn,
            crate::models::UpdateModDependenciesRequest {
                mod_id: "dependent".into(),
                entries: vec![
                    crate::models::UpdateModDependencyEntry {
                        dependency_type: "mod".into(),
                        target_mod_id: Some("required".into()),
                        target_value: "Required Mod".into(),
                        notes: String::new(),
                    },
                    crate::models::UpdateModDependencyEntry {
                        dependency_type: "plugin".into(),
                        target_mod_id: None,
                        target_value: "MissingMaster.esm".into(),
                        notes: "manual check".into(),
                    },
                ],
            },
        )
        .expect("save dependencies");

        let dependencies = list_mod_dependencies(&conn, "dependent").expect("list dependencies");

        assert_eq!(dependencies.len(), 2);
        let required_mod = dependencies
            .iter()
            .find(|dependency| dependency.dependency_type == "mod")
            .expect("mod dependency");
        assert!(required_mod.satisfied);
        assert_eq!(required_mod.status, "satisfied");

        let missing_plugin = dependencies
            .iter()
            .find(|dependency| dependency.dependency_type == "plugin")
            .expect("plugin dependency");
        assert!(!missing_plugin.satisfied);
        assert_eq!(missing_plugin.status, "missing");
    }

    #[test]
    fn dependency_summary_reports_missing_counts_per_mod() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('dependent', 'instance-a', 'Dependent Mod', '/mods/dependent', '/mods/dependent', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('required', 'instance-a', 'Required Mod', '/mods/required', '/mods/required', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('plain', 'instance-a', 'Plain Mod', '/mods/plain', '/mods/plain', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mods");
        update_mod_dependencies(
            &mut conn,
            crate::models::UpdateModDependenciesRequest {
                mod_id: "dependent".into(),
                entries: vec![
                    crate::models::UpdateModDependencyEntry {
                        dependency_type: "mod".into(),
                        target_mod_id: Some("required".into()),
                        target_value: "Required Mod".into(),
                        notes: String::new(),
                    },
                    crate::models::UpdateModDependencyEntry {
                        dependency_type: "plugin".into(),
                        target_mod_id: None,
                        target_value: "MissingMaster.esm".into(),
                        notes: String::new(),
                    },
                ],
            },
        )
        .expect("save dependencies");

        let summaries =
            list_instance_dependency_summary(&conn, "instance-a").expect("list dependency summary");
        let dependent = summaries
            .iter()
            .find(|entry| entry.mod_id == "dependent")
            .expect("dependent summary");
        let plain = summaries
            .iter()
            .find(|entry| entry.mod_id == "plain")
            .expect("plain summary");

        assert_eq!(dependent.dependency_count, 2);
        assert_eq!(dependent.missing_count, 1);
        assert_eq!(dependent.status, "missing");
        assert_eq!(plain.dependency_count, 0);
        assert_eq!(plain.missing_count, 0);
        assert_eq!(plain.status, "ok");
    }

    #[test]
    fn import_mod_folder_creates_plugin_master_dependencies() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");

        let workspace_root = temp_workspace("plugin-master-workspace");
        let source_root = temp_workspace("plugin-master-source");
        fs::create_dir_all(&source_root).expect("create source");
        fs::write(
            source_root.join("Patch.esp"),
            synthetic_plugin(&["Skyrim.esm", "MissingMaster.esm"]),
        )
        .expect("write plugin");

        let report = import_mod_folder(
            &mut conn,
            &workspace_root,
            ImportModFolderRequest {
                instance_id: "instance-a".into(),
                profile_id: None,
                target_mod_id: None,
                preview_source_path: None,
                name: "Patch Mod".into(),
                source_path: source_root.clone(),
                copy_into_workspace: true,
                fomod_selection: None,
            },
        )
        .expect("import mod");

        let dependencies =
            list_mod_dependencies(&conn, &report.mod_record.id).expect("list dependencies");

        assert!(dependencies.iter().any(|dependency| {
            dependency.dependency_type == "plugin" && dependency.target_value == "Skyrim.esm"
        }));
        assert!(dependencies.iter().any(|dependency| {
            dependency.dependency_type == "plugin" && dependency.target_value == "MissingMaster.esm"
        }));

        let _ = fs::remove_dir_all(workspace_root);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn plugin_dependencies_are_satisfied_by_files_in_instance_data_path() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");

        let workspace_root = temp_workspace("plugin-data-path-workspace");
        let install_path = temp_workspace("plugin-data-path-install");
        let data_path = install_path.join("Data");
        fs::create_dir_all(&data_path).expect("create data path");
        fs::write(data_path.join("Skyrim.esm"), b"base game").expect("write base esm");

        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', ?1, ?2, 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![install_path.to_string_lossy().to_string(), data_path.to_string_lossy().to_string()],
        )
        .expect("insert instance");

        let source_root = temp_workspace("plugin-data-path-source");
        fs::create_dir_all(&source_root).expect("create source");
        fs::write(
            source_root.join("Patch.esp"),
            synthetic_plugin(&["Skyrim.esm", "MissingMaster.esm"]),
        )
        .expect("write plugin");

        let report = import_mod_folder(
            &mut conn,
            &workspace_root,
            ImportModFolderRequest {
                instance_id: "instance-a".into(),
                profile_id: None,
                target_mod_id: None,
                preview_source_path: None,
                name: "Patch Mod".into(),
                source_path: source_root.clone(),
                copy_into_workspace: true,
                fomod_selection: None,
            },
        )
        .expect("import mod");

        let dependencies =
            list_mod_dependencies(&conn, &report.mod_record.id).expect("list dependencies");
        let skyrim = dependencies
            .iter()
            .find(|dependency| dependency.target_value == "Skyrim.esm")
            .expect("skyrim dependency");
        let missing = dependencies
            .iter()
            .find(|dependency| dependency.target_value == "MissingMaster.esm")
            .expect("missing dependency");

        assert!(skyrim.satisfied);
        assert_eq!(skyrim.status, "satisfied");
        assert!(!missing.satisfied);
        assert_eq!(missing.status, "missing");

        let _ = fs::remove_dir_all(workspace_root);
        let _ = fs::remove_dir_all(install_path);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn plugin_dependencies_normalize_data_paths_and_standard_masters() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");

        let install_path = temp_workspace("plugin-normalized-install");
        let data_path = install_path.join("Data");
        fs::create_dir_all(&data_path).expect("create data path");
        fs::write(data_path.join("PatchMaster.esm"), b"master").expect("write patch master");

        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', ?1, ?2, 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![install_path.to_string_lossy().to_string(), data_path.to_string_lossy().to_string()],
        )
        .expect("insert instance");

        assert!(
            dependency_plugin_satisfied(&conn, "instance-a", None, "Data/PatchMaster.esm")
                .expect("path plugin dependency")
        );
        assert!(
            dependency_plugin_satisfied(&conn, "instance-a", None, "*Skyrim.esm")
                .expect("standard master dependency")
        );
        assert!(
            !dependency_plugin_satisfied(&conn, "instance-a", None, "ccBGSSSE016-Umbra.esm")
                .expect("missing creation club master dependency")
        );

        let _ = fs::remove_dir_all(install_path);
    }

    #[test]
    fn import_fomod_creates_file_dependencies_from_module_dependencies() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        let game_root = temp_workspace("fomod-dependency-game");
        let game_data = game_root.join("Data");
        fs::create_dir_all(game_data.join("SKSE")).expect("create game dependency folder");
        fs::write(game_data.join("SKSE/skse64_loader.exe"), b"skse").expect("write dependency");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', ?1, ?2, 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![game_root.to_string_lossy().to_string(), game_data.to_string_lossy().to_string()],
        )
        .expect("insert instance");

        let workspace_root = temp_workspace("fomod-dependency-workspace");
        let source_root = temp_workspace("fomod-dependency-source");
        fs::create_dir_all(source_root.join("fomod")).expect("create fomod dir");
        fs::create_dir_all(source_root.join("Data")).expect("create data dir");
        fs::write(
            source_root.join("Data").join("Installed.esp"),
            synthetic_plugin(&[]),
        )
        .expect("write plugin");
        fs::write(
            source_root.join("fomod").join("ModuleConfig.xml"),
            r#"
            <config>
              <moduleName>FOMOD Dependency Test</moduleName>
              <moduleDependencies operator="And">
                <fileDependency file="SKSE/skse64_loader.exe" state="Active" />
              </moduleDependencies>
              <requiredInstallFiles>
                <file source="Data/Installed.esp" destination="Installed.esp" />
              </requiredInstallFiles>
            </config>
            "#,
        )
        .expect("write module config");

        let report = import_mod_folder(
            &mut conn,
            &workspace_root,
            ImportModFolderRequest {
                instance_id: "instance-a".into(),
                profile_id: None,
                target_mod_id: None,
                preview_source_path: None,
                name: "FOMOD Mod".into(),
                source_path: source_root.clone(),
                copy_into_workspace: true,
                fomod_selection: None,
            },
        )
        .expect("import fomod");

        let dependencies =
            list_mod_dependencies(&conn, &report.mod_record.id).expect("list dependencies");

        assert!(dependencies.iter().any(|dependency| {
            dependency.dependency_type == "file"
                && dependency.target_value == "skse/skse64_loader.exe"
        }));

        let _ = fs::remove_dir_all(workspace_root);
        let _ = fs::remove_dir_all(source_root);
        let _ = fs::remove_dir_all(game_root);
    }

    #[test]
    fn dependency_file_satisfied_detects_base_game_files_in_data_path() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");

        let workspace_root = temp_workspace("fomod-base-files");
        let data_path = workspace_root.join("Skyrim Data");
        fs::create_dir_all(&data_path).expect("create data path");
        fs::write(data_path.join("Skyrim.esm"), b"base game").expect("write base file");

        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', ?1, ?1, 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![data_path.to_string_lossy().to_string()],
        )
        .expect("insert instance");

        assert!(
            dependency_file_satisfied(&conn, "instance-a", None, "Skyrim.esm")
                .expect("check dependency")
        );
        assert!(
            dependency_file_satisfied(&conn, "instance-a", None, "Data/Skyrim.esm")
                .expect("check dependency with data prefix")
        );

        let _ = fs::remove_dir_all(workspace_root);
    }

    fn write_test_zip(path: &Path, files: &[(&str, &[u8])]) {
        let file = fs::File::create(path).expect("create zip");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in files {
            archive.start_file(*name, options).expect("start zip file");
            archive.write_all(content).expect("write zip file");
        }
        archive.finish().expect("finish zip");
    }

    fn synthetic_plugin(masters: &[&str]) -> Vec<u8> {
        let mut subrecords = Vec::new();
        for master in masters {
            subrecords.extend_from_slice(b"MAST");
            subrecords.extend_from_slice(&(master.len() as u16 + 1).to_le_bytes());
            subrecords.extend_from_slice(master.as_bytes());
            subrecords.push(0);
        }

        let mut plugin = Vec::new();
        plugin.extend_from_slice(b"TES4");
        plugin.extend_from_slice(&(subrecords.len() as u32).to_le_bytes());
        plugin.extend_from_slice(&[0; 16]);
        plugin.extend_from_slice(&subrecords);
        plugin
    }

    #[test]
    #[ignore = "requires SLIMCC_LIVE_FOMOD_PATH"]
    fn live_fomod_preview_uses_the_production_extractor_and_parser() {
        let archive = PathBuf::from(
            std::env::var("SLIMCC_LIVE_FOMOD_PATH").expect("live FOMOD archive path"),
        );
        let workspace = temp_workspace("live-fomod-preview");
        let preview = preview_fomod_package_with_context(&workspace, &archive, None, None)
            .expect("preview through production backend");
        assert!(preview.has_fomod);
        assert!(!preview.steps.is_empty() || !preview.required_files.is_empty());
        let groups: usize = preview.steps.iter().map(|step| step.groups.len()).sum();
        let options: usize = preview
            .steps
            .iter()
            .flat_map(|step| &step.groups)
            .map(|group| group.options.len())
            .sum();
        eprintln!(
            "FOMOD preview: steps={}, groups={}, options={}, notes={}",
            preview.steps.len(),
            groups,
            options,
            preview.validation_notes.len()
        );
        if std::env::var("SLIMCC_LIVE_FOMOD_IMPORT").as_deref() == Ok("1") {
            let mut conn = Connection::open_in_memory().expect("open in-memory database");
            conn.execute_batch(include_str!("../migrations/001_initial.sql"))
                .expect("create schema");
            conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
                .expect("create editor schema");
            conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
                .expect("create dependency schema");
            conn.execute(
                "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
                 VALUES ('instance-live', 'Skyrim Test', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("insert isolated instance");
            let report = import_mod_folder(
                &mut conn,
                &workspace,
                ImportModFolderRequest {
                    instance_id: "instance-live".into(),
                    profile_id: None,
                    target_mod_id: None,
                    preview_source_path: None,
                    name: "Live FOMOD Test".into(),
                    source_path: archive.clone(),
                    copy_into_workspace: true,
                    fomod_selection: None,
                },
            )
            .expect("import through production backend");
            assert!(report.files_scanned > 0);
            assert!(report.mod_record.installed_path.is_dir());
            eprintln!(
                "FOMOD import: files={}, plugins={}",
                report.files_scanned, report.plugins_discovered
            );
        }
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    #[ignore = "requires SLIMCC_LIVE_MOD_PATH"]
    fn live_mod_import_uses_the_production_extractor_and_scanner() {
        let archive =
            PathBuf::from(std::env::var("SLIMCC_LIVE_MOD_PATH").expect("live mod archive path"));
        let workspace = temp_workspace("live-mod-import");
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-live', 'Skyrim Test', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert isolated instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-live', 'instance-live', 'Test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert isolated profile");
        let report = import_mod_folder(
            &mut conn,
            &workspace,
            ImportModFolderRequest {
                instance_id: "instance-live".into(),
                profile_id: Some("profile-live".into()),
                target_mod_id: None,
                preview_source_path: None,
                name: "Live Mod Test".into(),
                source_path: archive,
                copy_into_workspace: true,
                fomod_selection: None,
            },
        )
        .expect("import through production backend");
        assert!(report.files_scanned > 0);
        assert!(report.mod_record.installed_path.is_dir());
        eprintln!(
            "Mod import: files={}, plugins={}",
            report.files_scanned, report.plugins_discovered
        );
        let plan = crate::deploy::build_plan(
            &conn,
            &workspace,
            "instance-live",
            "profile-live",
            crate::models::DeployTarget::Staging,
            crate::models::DeployAction::Symlink,
        )
        .expect("build VFS plan");
        crate::staging::execute_staging_plan(&workspace, &plan).expect("stage VFS layer");
        let game_root_operations = plan
            .operations
            .iter()
            .filter(|operation| {
                operation
                    .original_rel_path
                    .starts_with(crate::scanner::GAME_ROOT_PREFIX)
            })
            .count();
        eprintln!(
            "VFS plan: operations={}, game_root={game_root_operations}",
            plan.operations.len()
        );
        for operation in plan.operations.iter().filter(|operation| {
            operation
                .original_rel_path
                .starts_with(crate::scanner::GAME_ROOT_PREFIX)
        }) {
            assert!(operation.target.starts_with(crate::paths::vfs_layer_path(
                &workspace,
                "instance-live",
                "profile-live"
            )));
            assert!(!operation
                .target
                .starts_with(crate::paths::vfs_layer_data_path(
                    &workspace,
                    "instance-live",
                    "profile-live"
                )));
        }
        let _ = std::fs::remove_dir_all(workspace);
    }
}

pub fn preview_fomod_package_with_context(
    workspace_root: &Path,
    package_root: &Path,
    saved_selection: Option<crate::models::FomodSelectionRequest>,
    dependency_context: Option<&FomodDependencyContext>,
) -> SlimResult<crate::models::FomodPackagePreview> {
    if package_root.is_file() {
        let Some(kind) = ArchiveKind::from_path(package_root) else {
            return fomod::inspect_package_with_context(
                package_root,
                saved_selection,
                dependency_context,
            );
        };

        let preview_root = workspace_root
            .join("fomod-preview")
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&preview_root)?;
        paths::guard_descendant(workspace_root, &preview_root.join(".guard"))?;
        extract_archive(package_root, &preview_root, kind)?;

        let preview = fomod::inspect_package_with_context(
            &preview_root,
            saved_selection,
            dependency_context,
        )?;
        if !preview.has_fomod {
            let _ = fs::remove_dir_all(&preview_root);
        }
        return Ok(preview);
    }

    fomod::inspect_package_with_context(package_root, saved_selection, dependency_context)
}

pub fn preview_mod_fomod(
    conn: &Connection,
    mod_id: &str,
) -> SlimResult<crate::models::FomodPackagePreview> {
    let (instance_id, source_path, saved_selection) = conn.query_row(
        "SELECT m.instance_id, m.source_path, mi.state_json
         FROM mods m
         LEFT JOIN mod_installers mi ON mi.mod_id = m.id
         WHERE m.id = ?1",
        params![mod_id],
        |row| {
            let instance_id: String = row.get(0)?;
            let source_path: String = row.get(1)?;
            let state_json: Option<String> = row.get(2)?;
            Ok((instance_id, PathBuf::from(source_path), state_json))
        },
    )?;

    let workspace_root = paths::workspace_root()?;
    let dependency_context = build_fomod_dependency_context(conn, &instance_id)?;
    match saved_selection {
        Some(state_json) => {
            if let Ok(state) = serde_json::from_str::<FomodInstallerState>(&state_json) {
                if state.source_root.is_file() {
                    preview_fomod_package_with_context(
                        &workspace_root,
                        &state.source_root,
                        Some(state.selection),
                        Some(&dependency_context),
                    )
                } else {
                    fomod::inspect_package_with_context(
                        &state.source_root,
                        Some(state.selection),
                        Some(&dependency_context),
                    )
                }
            } else {
                let selection = serde_json::from_str(&state_json)?;
                if source_path.is_file() {
                    preview_fomod_package_with_context(
                        &workspace_root,
                        &source_path,
                        Some(selection),
                        Some(&dependency_context),
                    )
                } else {
                    fomod::inspect_package_with_context(
                        &source_path,
                        Some(selection),
                        Some(&dependency_context),
                    )
                }
            }
        }
        None => {
            if source_path.is_file() {
                preview_fomod_package_with_context(
                    &workspace_root,
                    &source_path,
                    None,
                    Some(&dependency_context),
                )
            } else {
                fomod::inspect_package_with_context(&source_path, None, Some(&dependency_context))
            }
        }
    }
}
