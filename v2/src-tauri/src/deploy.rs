use crate::error::{SlimError, SlimResult};
use crate::instance;
use crate::models::{
    ConflictCandidate, ConflictEntry, DeployAction, DeployOperation, DeployPlan, DeployTarget,
    DeployWarning,
};
use crate::paths;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug)]
struct CandidateRow {
    mod_id: String,
    priority: i64,
    original_rel_path: String,
    normalized_rel_path: String,
    abs_source_path: String,
}

pub fn build_plan(
    conn: &Connection,
    workspace_root: &Path,
    instance_id: &str,
    profile_id: &str,
    target: DeployTarget,
    action: DeployAction,
) -> SlimResult<DeployPlan> {
    crate::mods::ensure_plugin_index_for_instance(conn, instance_id)?;
    let rows = load_enabled_file_candidates(conn, instance_id, profile_id)?;
    let target_root = plan_target_root(conn, workspace_root, instance_id, profile_id, &target)?;
    let mut grouped: BTreeMap<String, Vec<CandidateRow>> = BTreeMap::new();

    for row in rows {
        grouped
            .entry(row.normalized_rel_path.clone())
            .or_default()
            .push(row);
    }

    let mut operations = Vec::new();
    let mut emitted_file_paths = BTreeSet::new();
    let mut conflicts = Vec::new();
    let mut warnings = collect_dependency_warnings(conn, instance_id, profile_id)?;
    for (normalized_rel_path, mut candidates) in grouped {
        candidates.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| b.mod_id.cmp(&a.mod_id))
        });

        let winner = candidates
            .first()
            .ok_or_else(|| SlimError::NotFound("candidate group had no winner".into()))?;

        if !Path::new(&winner.abs_source_path).exists() {
            warnings.push(DeployWarning {
                mod_id: winner.mod_id.clone(),
                dependency_id: winner.normalized_rel_path.clone(),
                dependency_type: "source_file".into(),
                target_value: winner.original_rel_path.clone(),
                message: format!(
                    "Skipping '{}' from mod '{}' because the source file no longer exists: {}",
                    winner.original_rel_path, winner.mod_id, winner.abs_source_path
                ),
            });
            continue;
        }

        if let Some(parent_path) =
            planned_file_ancestor(&winner.normalized_rel_path, &emitted_file_paths)
        {
            warnings.push(DeployWarning {
                mod_id: winner.mod_id.clone(),
                dependency_id: winner.normalized_rel_path.clone(),
                dependency_type: "path_collision".into(),
                target_value: winner.original_rel_path.clone(),
                message: format!(
                    "Skipping '{}' from mod '{}' because '{}' is already planned as a file and cannot also be a directory",
                    winner.original_rel_path, winner.mod_id, parent_path
                ),
            });
            continue;
        }

        let target_path = target_root.join(&winner.original_rel_path);

        operations.push(DeployOperation {
            action: action.clone(),
            source: winner.abs_source_path.clone().into(),
            target: target_path,
            mod_id: winner.mod_id.clone(),
            original_rel_path: winner.original_rel_path.clone(),
            normalized_rel_path: winner.normalized_rel_path.clone(),
            conflict_winner: candidates.len() > 1,
        });
        emitted_file_paths.insert(winner.normalized_rel_path.clone());

        if candidates.len() > 1 {
            conflicts.push(ConflictEntry {
                normalized_rel_path,
                winner_mod_id: winner.mod_id.clone(),
                candidates: candidates
                    .into_iter()
                    .map(|c| ConflictCandidate {
                        mod_id: c.mod_id,
                        priority: c.priority,
                        original_rel_path: c.original_rel_path,
                        abs_source_path: c.abs_source_path.into(),
                    })
                    .collect(),
            });
        }
    }

    Ok(DeployPlan {
        instance_id: instance_id.to_string(),
        profile_id: profile_id.to_string(),
        target,
        generated_at: Utc::now().to_rfc3339(),
        operations,
        conflicts,
        warnings,
    })
}

fn planned_file_ancestor(path: &str, emitted_file_paths: &BTreeSet<String>) -> Option<String> {
    let mut parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    while parts.len() > 1 {
        parts.pop();
        let parent = parts.join("/");
        if emitted_file_paths.contains(&parent) {
            return Some(parent);
        }
    }
    None
}

fn plan_target_root(
    conn: &Connection,
    workspace_root: &Path,
    instance_id: &str,
    profile_id: &str,
    target: &DeployTarget,
) -> SlimResult<std::path::PathBuf> {
    match target {
        DeployTarget::DryRun | DeployTarget::Staging => Ok(paths::vfs_layer_data_path(
            workspace_root,
            instance_id,
            profile_id,
        )),
        DeployTarget::RealData => Ok(instance::get_instance_by_id(conn, instance_id)?.data_path),
    }
}

fn collect_dependency_warnings(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<Vec<DeployWarning>> {
    let mut stmt = conn.prepare(
        "SELECT m.id
         FROM mods m
         LEFT JOIN profile_mods pm ON pm.mod_id = m.id AND pm.profile_id = ?2
         WHERE m.instance_id = ?1
           AND COALESCE(pm.enabled, m.enabled_default) = 1",
    )?;
    let rows = stmt.query_map(params![instance_id, profile_id], |row| {
        row.get::<_, String>(0)
    })?;

    let mut warnings = Vec::new();
    for row in rows {
        let mod_id = row?;
        for dependency in
            crate::mods::list_mod_dependencies_for_profile(conn, &mod_id, Some(profile_id))?
        {
            if dependency.satisfied {
                continue;
            }
            warnings.push(DeployWarning {
                mod_id: mod_id.clone(),
                dependency_id: dependency.id,
                dependency_type: dependency.dependency_type.clone(),
                target_value: dependency.target_value.clone(),
                message: format!(
                    "Mod '{}' has missing {} dependency '{}'",
                    mod_id, dependency.dependency_type, dependency.target_value
                ),
            });
        }
    }

    warnings.extend(collect_rule_warnings(conn, instance_id, profile_id)?);

    Ok(warnings)
}

fn collect_rule_warnings(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<Vec<DeployWarning>> {
    let profile_mods = crate::mods::list_profile_mods(conn, instance_id, profile_id)?;
    let mut warnings = Vec::new();

    for entry in profile_mods {
        match entry.rule_type.as_deref() {
            Some("force_enable") if !entry.enabled => warnings.push(DeployWarning {
                mod_id: entry.mod_id.clone(),
                dependency_id: entry.mod_id.clone(),
                dependency_type: "rule".into(),
                target_value: "force_enable".into(),
                message: format!(
                    "Mod '{}' is disabled in this profile but marked force_enable",
                    entry.mod_name
                ),
            }),
            Some("force_disable") if entry.enabled => warnings.push(DeployWarning {
                mod_id: entry.mod_id.clone(),
                dependency_id: entry.mod_id.clone(),
                dependency_type: "rule".into(),
                target_value: "force_disable".into(),
                message: format!(
                    "Mod '{}' is enabled in this profile but marked force_disable",
                    entry.mod_name
                ),
            }),
            Some("prefer") | Some("defer") => {
                if entry.rule_target_mod_id.is_none() && entry.rule_weight > 0 {
                    warnings.push(DeployWarning {
                        mod_id: entry.mod_id.clone(),
                        dependency_id: entry.mod_id.clone(),
                        dependency_type: "rule".into(),
                        target_value: entry.rule_type.clone().unwrap_or_default(),
                        message: format!(
                            "Mod '{}' has a weighted local rule without a target mod",
                            entry.mod_name
                        ),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(warnings)
}

fn load_enabled_file_candidates(
    conn: &Connection,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<Vec<CandidateRow>> {
    let mut stmt = conn.prepare(
        "SELECT mf.mod_id, COALESCE(pm.priority, 0), mf.original_rel_path, mf.normalized_rel_path, mf.abs_source_path
         FROM mod_files mf
         JOIN mods m ON m.id = mf.mod_id
         LEFT JOIN profile_mods pm ON pm.mod_id = m.id AND pm.profile_id = ?2
         LEFT JOIN plugins p ON p.mod_id = mf.mod_id AND p.normalized_rel_path = mf.normalized_rel_path
         LEFT JOIN profile_plugins pp ON pp.plugin_id = p.id AND pp.profile_id = ?2
         WHERE m.instance_id = ?1
           AND COALESCE(pm.enabled, m.enabled_default) = 1
           AND (p.id IS NULL OR COALESCE(pp.enabled, 1) = 1)",
    )?;

    let rows = stmt.query_map(params![instance_id, profile_id], |row| {
        Ok(CandidateRow {
            mod_id: row.get(0)?,
            priority: row.get(1)?,
            original_rel_path: row.get(2)?,
            normalized_rel_path: row.get(3)?,
            abs_source_path: row.get(4)?,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{UpdateModDependenciesRequest, UpdateModDependencyEntry};
    use rusqlite::Connection;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn build_plan_skips_disabled_plugin_but_keeps_other_mod_files() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        let workspace_root = PathBuf::from(
            std::env::temp_dir().join(format!("slimcc-profile-plugin-{}", Uuid::new_v4())),
        );
        let source_root = workspace_root.join("source");
        fs::create_dir_all(source_root.join("meshes")).expect("create source root");
        fs::write(source_root.join("Patch.esp"), b"plugin").expect("write plugin");
        fs::write(source_root.join("meshes").join("item.nif"), b"mesh").expect("write mesh");

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
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, enabled_default, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', ?1, ?1, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [source_root.to_string_lossy().to_string()],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'mod-a', 1, 10)",
            [],
        )
        .expect("enable mod");
        conn.execute(
            "INSERT INTO mod_files (id, mod_id, original_rel_path, normalized_rel_path, abs_source_path, created_at)
             VALUES ('file-plugin', 'mod-a', 'Patch.esp', 'patch.esp', ?1, '2026-01-01T00:00:00Z'),
                    ('file-mesh', 'mod-a', 'meshes/item.nif', 'meshes/item.nif', ?2, '2026-01-01T00:00:00Z')",
            params![
                source_root.join("Patch.esp").to_string_lossy().to_string(),
                source_root.join("meshes").join("item.nif").to_string_lossy().to_string()
            ],
        )
        .expect("insert files");
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES ('plugin-patch', 'mod-a', 'Patch.esp', 'esp', 'patch.esp', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert plugin");
        conn.execute(
            "INSERT INTO profile_plugins (profile_id, plugin_id, enabled, updated_at)
             VALUES ('profile-a', 'plugin-patch', 0, '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("disable plugin");

        let plan = build_plan(
            &conn,
            &workspace_root,
            "instance-a",
            "profile-a",
            DeployTarget::DryRun,
            DeployAction::Copy,
        )
        .expect("build deploy plan");

        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].normalized_rel_path, "meshes/item.nif");

        let _ = fs::remove_dir_all(&workspace_root);
    }

    #[test]
    fn deploy_plan_warns_about_missing_enabled_mod_dependencies() {
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
             VALUES ('dependent', 'instance-a', 'Dependent Mod', '/mods/dependent', '/mods/dependent', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'dependent', 1, 0)",
            [],
        )
        .expect("enable mod");
        crate::mods::update_mod_dependencies(
            &mut conn,
            UpdateModDependenciesRequest {
                mod_id: "dependent".into(),
                entries: vec![UpdateModDependencyEntry {
                    dependency_type: "plugin".into(),
                    target_mod_id: None,
                    target_value: "MissingMaster.esm".into(),
                    notes: String::new(),
                }],
            },
        )
        .expect("save dependencies");

        let plan = build_plan(
            &conn,
            &PathBuf::from("/tmp/workspace"),
            "instance-a",
            "profile-a",
            DeployTarget::DryRun,
            DeployAction::Copy,
        )
        .expect("build deploy plan");

        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].mod_id, "dependent");
        assert_eq!(plan.warnings[0].dependency_type, "plugin");
        assert_eq!(plan.warnings[0].target_value, "MissingMaster.esm");
    }

    #[test]
    fn build_plan_includes_default_enabled_mods_without_profile_rows() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        let workspace_root = PathBuf::from(
            std::env::temp_dir().join(format!("slimcc-deploy-plan-{}", Uuid::new_v4())),
        );
        let source_root = workspace_root.join("source");
        fs::create_dir_all(&source_root).expect("create source root");
        fs::write(source_root.join("plugin.esp"), b"data").expect("write source file");

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
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, enabled_default, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', ?1, ?1, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [source_root.to_string_lossy().to_string()],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO mod_files (id, mod_id, original_rel_path, normalized_rel_path, abs_source_path, created_at)
             VALUES ('file-a', 'mod-a', 'plugin.esp', 'plugin.esp', ?1, '2026-01-01T00:00:00Z')",
            [source_root.join("plugin.esp").to_string_lossy().to_string()],
        )
        .expect("insert mod file");

        let plan = build_plan(
            &conn,
            &workspace_root,
            "instance-a",
            "profile-a",
            DeployTarget::DryRun,
            DeployAction::Copy,
        )
        .expect("build deploy plan");

        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].mod_id, "mod-a");
        assert_eq!(
            plan.operations[0].target,
            workspace_root
                .join("instances/instance-a/profiles/profile-a/vfs/layer/Data/plugin.esp")
        );

        let _ = fs::remove_dir_all(&workspace_root);
    }

    #[test]
    fn build_plan_skips_missing_source_files_with_warning() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        let workspace_root = PathBuf::from(
            std::env::temp_dir().join(format!("slimcc-missing-source-{}", Uuid::new_v4())),
        );
        let source_root = workspace_root.join("missing-source");
        let missing_file = source_root.join("plugin.esp");

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
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, enabled_default, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', ?1, ?1, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [source_root.to_string_lossy().to_string()],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO mod_files (id, mod_id, original_rel_path, normalized_rel_path, abs_source_path, created_at)
             VALUES ('file-a', 'mod-a', 'plugin.esp', 'plugin.esp', ?1, '2026-01-01T00:00:00Z')",
            [missing_file.to_string_lossy().to_string()],
        )
        .expect("insert mod file");

        let plan = build_plan(
            &conn,
            &workspace_root,
            "instance-a",
            "profile-a",
            DeployTarget::DryRun,
            DeployAction::Copy,
        )
        .expect("build deploy plan");

        assert!(plan.operations.is_empty());
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].dependency_type, "source_file");
        assert!(plan.warnings[0]
            .message
            .contains("source file no longer exists"));

        let _ = fs::remove_dir_all(&workspace_root);
    }

    #[test]
    fn staging_skips_nested_path_when_parent_file_is_planned() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        let workspace_root = PathBuf::from(
            std::env::temp_dir().join(format!("slimcc-path-collision-{}", Uuid::new_v4())),
        );
        let source_root = workspace_root.join("source");
        fs::create_dir_all(source_root.join("interface")).expect("create nested source root");
        fs::write(source_root.join("interface-file"), b"parent").expect("write parent source");
        fs::write(source_root.join("interface").join("child.txt"), b"child")
            .expect("write child source");

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
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, enabled_default, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', ?1, ?1, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [source_root.to_string_lossy().to_string()],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO mod_files (id, mod_id, original_rel_path, normalized_rel_path, abs_source_path, created_at)
             VALUES ('file-parent', 'mod-a', 'interface', 'interface', ?1, '2026-01-01T00:00:00Z'),
                    ('file-child', 'mod-a', 'interface/child.txt', 'interface/child.txt', ?2, '2026-01-01T00:00:00Z')",
            params![
                source_root.join("interface-file").to_string_lossy().to_string(),
                source_root.join("interface").join("child.txt").to_string_lossy().to_string()
            ],
        )
        .expect("insert mod files");

        let plan = build_plan(
            &conn,
            &workspace_root,
            "instance-a",
            "profile-a",
            DeployTarget::Staging,
            DeployAction::Copy,
        )
        .expect("build deploy plan");

        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].normalized_rel_path, "interface");
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].dependency_type, "path_collision");
        crate::staging::execute_staging_plan(&workspace_root, &plan).expect("execute staging");
        assert!(workspace_root
            .join("instances/instance-a/profiles/profile-a/vfs/layer/Data/interface")
            .is_file());

        let _ = fs::remove_dir_all(&workspace_root);
    }

    #[test]
    fn build_plan_uses_instance_data_path_for_real_data_target() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        let workspace_root = PathBuf::from(
            std::env::temp_dir().join(format!("slimcc-real-deploy-{}", Uuid::new_v4())),
        );
        let source_file_root = workspace_root.join("source");
        let data_root = workspace_root.join("live-data");
        fs::create_dir_all(&source_file_root).expect("create source root");
        fs::create_dir_all(&data_root).expect("create data root");
        fs::write(source_file_root.join("plugin.esp"), b"data").expect("write source file");

        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', ?1, 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [data_root.to_string_lossy().to_string()],
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
             VALUES ('mod-a', 'instance-a', 'Mod A', ?1, ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [source_file_root.to_string_lossy().to_string()],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('profile-a', 'mod-a', 1, 0)",
            [],
        )
        .expect("enable mod");
        conn.execute(
            "INSERT INTO mod_files (id, mod_id, original_rel_path, normalized_rel_path, abs_source_path, created_at)
             VALUES ('file-a', 'mod-a', 'Data/plugin.esp', 'data/plugin.esp', ?1, '2026-01-01T00:00:00Z')",
            [source_file_root.join("plugin.esp").to_string_lossy().to_string()],
        )
        .expect("insert mod file");

        let plan = build_plan(
            &conn,
            &workspace_root,
            "instance-a",
            "profile-a",
            DeployTarget::RealData,
            DeployAction::Copy,
        )
        .expect("build real data plan");

        assert_eq!(plan.operations[0].target, data_root.join("Data/plugin.esp"));
        let _ = fs::remove_dir_all(&workspace_root);
    }
}
