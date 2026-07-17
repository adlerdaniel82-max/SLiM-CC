use crate::error::SlimResult;
use crate::models::{
    DeployAction, DeployTarget, DiagnosisReport, DiagnosticFinding, ModConflictSummary,
    ModDependencySummary, ProfileComparison, ProfileModEntry, ProfilePriorityChange,
    StoredDiagnosticFinding, SuspectModScore, ToolRunRecord,
};
use chrono::Utc;
use rusqlite::{params, Connection, Row};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub fn build_diagnosis_report(
    conn: &Connection,
    workspace_root: &Path,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<DiagnosisReport> {
    let deploy_plan = crate::deploy::build_plan(
        conn,
        workspace_root,
        instance_id,
        profile_id,
        DeployTarget::DryRun,
        DeployAction::Copy,
    )?;
    let mods = crate::mods::list_profile_mods(conn, instance_id, profile_id)?;
    let dependencies = crate::mods::list_instance_dependency_summary(conn, instance_id)?;
    let conflicts = crate::conflicts::summarize_mod_conflicts(&deploy_plan.conflicts);
    let recent_tool_runs = crate::tools::list_recent_tool_runs(conn, 10)?;
    let findings = resolve_finding_mod_ids(
        conn,
        instance_id,
        parse_tool_run_findings(&recent_tool_runs),
    )?;
    let stored_findings = list_stored_findings(conn, Some(instance_id), 50)?;
    let mut scoring_findings = findings.clone();
    scoring_findings.extend(stored_findings.iter().map(stored_to_finding));
    let suspect_mods = score_suspect_mods(
        &mods,
        &dependencies,
        &conflicts,
        &deploy_plan.warnings,
        &scoring_findings,
    );

    Ok(DiagnosisReport {
        instance_id: instance_id.to_string(),
        profile_id: profile_id.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        mods,
        dependencies,
        conflicts,
        deploy_warnings: deploy_plan.warnings,
        tool_validations: crate::tools::validate_tool_profiles(conn)?,
        recent_tool_runs,
        findings,
        stored_findings,
        suspect_mods,
    })
}

pub fn parse_tool_run_findings(runs: &[ToolRunRecord]) -> Vec<DiagnosticFinding> {
    let mut findings = Vec::new();
    for run in runs {
        let mut text = String::new();
        if let Some(stdout) = &run.stdout {
            text.push_str(stdout);
            text.push('\n');
        }
        if let Some(stderr) = &run.stderr {
            text.push_str(stderr);
        }
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let finding = classify_diagnostic_line(&run.tool_key, line);

            if let Some((severity, category)) = finding {
                findings.push(DiagnosticFinding {
                    source: run.tool_key.clone(),
                    severity: severity.into(),
                    category: category.into(),
                    mod_id: None,
                    plugin: extract_plugin_name(line),
                    message: line.to_string(),
                    evidence: line.to_string(),
                });
            }
        }
    }
    findings
}

pub fn import_diagnostic_logs(log_dir: &Path) -> SlimResult<Vec<DiagnosticFinding>> {
    let mut findings = Vec::new();
    if !log_dir.exists() {
        return Ok(findings);
    }
    for entry in fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !(name.ends_with(".log") || name.ends_with(".txt")) {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        for line in content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let category = classify_diagnostic_line(&name, line).or_else(|| {
                let lower = line.to_lowercase();
                if lower.contains("exception") || lower.contains("crash") {
                    Some(("critical", "crash_log"))
                } else if lower.contains("stack") || lower.contains("script") {
                    Some(("warning", "script_log"))
                } else if lower.contains("error") || lower.contains("fatal") {
                    Some(("warning", "log_error"))
                } else {
                    None
                }
            });
            if let Some((severity, category)) = category {
                findings.push(DiagnosticFinding {
                    source: name.clone(),
                    severity: severity.into(),
                    category: category.into(),
                    mod_id: None,
                    plugin: extract_plugin_name(line),
                    message: line.to_string(),
                    evidence: path.display().to_string(),
                });
            }
        }
    }
    Ok(findings)
}

pub fn import_and_store_diagnostic_logs(
    conn: &Connection,
    instance_id: Option<&str>,
    log_dir: &Path,
) -> SlimResult<Vec<StoredDiagnosticFinding>> {
    let findings = match instance_id {
        Some(instance_id) => {
            resolve_finding_mod_ids(conn, instance_id, import_diagnostic_logs(log_dir)?)?
        }
        None => import_diagnostic_logs(log_dir)?,
    };
    store_findings(conn, instance_id, &findings)
}

pub fn store_findings(
    conn: &Connection,
    instance_id: Option<&str>,
    findings: &[DiagnosticFinding],
) -> SlimResult<Vec<StoredDiagnosticFinding>> {
    let now = Utc::now().to_rfc3339();
    for finding in findings {
        conn.execute(
            "INSERT INTO diagnostic_findings (
                id, instance_id, source, severity, category, mod_id, plugin, message, evidence, found_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                Uuid::new_v4().to_string(),
                instance_id,
                finding.source,
                finding.severity,
                finding.category,
                finding.mod_id,
                finding.plugin,
                finding.message,
                finding.evidence,
                now,
            ],
        )?;
    }
    list_stored_findings(conn, instance_id, 50)
}

pub fn list_stored_findings(
    conn: &Connection,
    instance_id: Option<&str>,
    limit: i64,
) -> SlimResult<Vec<StoredDiagnosticFinding>> {
    let limit = limit.max(1);
    if let Some(instance_id) = instance_id {
        let mut stmt = conn.prepare(
            "SELECT id, instance_id, source, severity, category, mod_id, plugin, message, evidence, found_at
             FROM diagnostic_findings
             WHERE instance_id = ?1
             ORDER BY found_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![instance_id, limit], row_to_stored_finding)?;
        return rows.collect::<Result<Vec<_>, _>>().map_err(Into::into);
    }

    let mut stmt = conn.prepare(
        "SELECT id, instance_id, source, severity, category, mod_id, plugin, message, evidence, found_at
         FROM diagnostic_findings
         WHERE instance_id IS NULL
         ORDER BY found_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], row_to_stored_finding)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn resolve_finding_mod_ids(
    conn: &Connection,
    instance_id: &str,
    findings: Vec<DiagnosticFinding>,
) -> SlimResult<Vec<DiagnosticFinding>> {
    findings
        .into_iter()
        .map(|mut finding| {
            if finding.mod_id.is_none() {
                if let Some(plugin) = &finding.plugin {
                    finding.mod_id = plugin_owner(conn, instance_id, plugin)?;
                }
            }
            Ok(finding)
        })
        .collect()
}

pub fn compare_profiles(
    conn: &Connection,
    instance_id: &str,
    base_profile_id: &str,
    compare_profile_id: &str,
) -> SlimResult<ProfileComparison> {
    let base_mods = crate::mods::list_profile_mods(conn, instance_id, base_profile_id)?;
    let compare_mods = crate::mods::list_profile_mods(conn, instance_id, compare_profile_id)?;
    let base_by_id: BTreeMap<_, _> = base_mods
        .iter()
        .map(|entry| (entry.mod_id.clone(), entry))
        .collect();
    let compare_by_id: BTreeMap<_, _> = compare_mods
        .iter()
        .map(|entry| (entry.mod_id.clone(), entry))
        .collect();

    let added_mods = compare_mods
        .iter()
        .filter(|entry| entry.enabled)
        .filter(|entry| {
            base_by_id
                .get(&entry.mod_id)
                .map(|base| !base.enabled)
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    let removed_mods = base_mods
        .iter()
        .filter(|entry| entry.enabled)
        .filter(|entry| {
            compare_by_id
                .get(&entry.mod_id)
                .map(|compare| !compare.enabled)
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    let changed_priorities = compare_mods
        .iter()
        .filter_map(|compare| {
            let base = base_by_id.get(&compare.mod_id)?;
            (base.priority != compare.priority).then(|| ProfilePriorityChange {
                mod_id: compare.mod_id.clone(),
                mod_name: compare.mod_name.clone(),
                base_priority: base.priority,
                compare_priority: compare.priority,
            })
        })
        .collect();

    Ok(ProfileComparison {
        instance_id: instance_id.to_string(),
        base_profile_id: base_profile_id.to_string(),
        compare_profile_id: compare_profile_id.to_string(),
        added_mods,
        removed_mods,
        changed_priorities,
    })
}

fn score_suspect_mods(
    mods: &[ProfileModEntry],
    dependencies: &[ModDependencySummary],
    conflicts: &[ModConflictSummary],
    warnings: &[crate::models::DeployWarning],
    findings: &[DiagnosticFinding],
) -> Vec<SuspectModScore> {
    let mut scores: BTreeMap<String, SuspectModScore> = BTreeMap::new();
    for entry in mods {
        scores.insert(
            entry.mod_id.clone(),
            SuspectModScore {
                mod_id: entry.mod_id.clone(),
                score: 0,
                reasons: Vec::new(),
            },
        );
    }
    for dependency in dependencies {
        if dependency.missing_count > 0 {
            add_score(
                &mut scores,
                &dependency.mod_id,
                dependency.missing_count * 100,
                format!("{} missing dependencies", dependency.missing_count),
            );
        }
    }
    for warning in warnings {
        add_score(&mut scores, &warning.mod_id, 120, warning.message.clone());
    }
    for conflict in conflicts {
        let mut weight = 5;
        if conflict.file_kinds.iter().any(|kind| kind == "scripts") {
            weight += 35;
        }
        if conflict.file_kinds.iter().any(|kind| kind == "behaviors") {
            weight += 45;
        }
        if conflict.file_kinds.iter().any(|kind| kind == "plugins") {
            weight += 50;
        }
        add_score(
            &mut scores,
            &conflict.mod_id,
            conflict.losing_file_count * weight,
            format!(
                "loses {} conflict files ({})",
                conflict.losing_file_count,
                conflict.file_kinds.join(", ")
            ),
        );
    }
    for finding in findings {
        if let Some(mod_id) = &finding.mod_id {
            let weight = match finding.severity.as_str() {
                "critical" => 150,
                "warning" => 60,
                _ => 15,
            };
            add_score(&mut scores, mod_id, weight, finding.message.clone());
        }
    }
    for entry in mods {
        match entry.rule_type.as_deref() {
            Some("force_enable") if !entry.enabled => {
                add_score(
                    &mut scores,
                    &entry.mod_id,
                    entry.rule_weight.max(100),
                    "force_enable rule conflicts with disabled profile entry".into(),
                );
            }
            Some("force_disable") if entry.enabled => {
                add_score(
                    &mut scores,
                    &entry.mod_id,
                    entry.rule_weight.max(100),
                    "force_disable rule conflicts with enabled profile entry".into(),
                );
            }
            Some("prefer") | Some("defer") => {
                if entry.rule_target_mod_id.as_deref().is_none() && entry.rule_weight > 0 {
                    add_score(
                        &mut scores,
                        &entry.mod_id,
                        entry.rule_weight.min(25),
                        format!(
                            "local rule '{}' has no target mod",
                            entry.rule_type.as_deref().unwrap_or("rule")
                        ),
                    );
                }
            }
            _ => {}
        }
    }
    let mut out: Vec<_> = scores
        .into_values()
        .filter(|entry| entry.score > 0)
        .collect();
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.mod_id.cmp(&b.mod_id)));
    out
}

fn row_to_stored_finding(row: &Row<'_>) -> rusqlite::Result<StoredDiagnosticFinding> {
    Ok(StoredDiagnosticFinding {
        id: row.get(0)?,
        instance_id: row.get(1)?,
        source: row.get(2)?,
        severity: row.get(3)?,
        category: row.get(4)?,
        mod_id: row.get(5)?,
        plugin: row.get(6)?,
        message: row.get(7)?,
        evidence: row.get(8)?,
        found_at: row.get(9)?,
    })
}

fn plugin_owner(conn: &Connection, instance_id: &str, plugin: &str) -> SlimResult<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT p.mod_id
         FROM plugins p
         JOIN mods m ON m.id = p.mod_id
         WHERE m.instance_id = ?1 AND lower(p.filename) = lower(?2)
         ORDER BY m.name ASC
         LIMIT 1",
    )?;
    let mut rows = stmt.query(params![instance_id, plugin])?;
    Ok(rows.next()?.map(|row| row.get(0)).transpose()?)
}

fn stored_to_finding(stored: &StoredDiagnosticFinding) -> DiagnosticFinding {
    DiagnosticFinding {
        source: stored.source.clone(),
        severity: stored.severity.clone(),
        category: stored.category.clone(),
        mod_id: stored.mod_id.clone(),
        plugin: stored.plugin.clone(),
        message: stored.message.clone(),
        evidence: stored.evidence.clone(),
    }
}

fn add_score(
    scores: &mut BTreeMap<String, SuspectModScore>,
    mod_id: &str,
    score: i64,
    reason: String,
) {
    let entry = scores
        .entry(mod_id.to_string())
        .or_insert_with(|| SuspectModScore {
            mod_id: mod_id.to_string(),
            score: 0,
            reasons: Vec::new(),
        });
    entry.score += score;
    if !reason.trim().is_empty() {
        entry.reasons.push(reason);
    }
}

fn extract_plugin_name(line: &str) -> Option<String> {
    line.split(|ch: char| {
        ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';' | ':' | '(' | ')')
    })
    .find(|token| {
        let lower = token.to_lowercase();
        lower.ends_with(".esp") || lower.ends_with(".esm") || lower.ends_with(".esl")
    })
    .map(|value| value.trim_matches(|ch| matches!(ch, '[' | ']')).to_string())
}

fn classify_diagnostic_line(source: &str, line: &str) -> Option<(&'static str, &'static str)> {
    let lower = line.to_lowercase();
    let source_lower = source.to_lowercase();
    if lower.contains("missing master") || lower.contains("requires master") {
        Some(("critical", "missing_master"))
    } else if lower.contains("cyclic interaction") || lower.contains("cycle detected") {
        Some(("critical", "loot_cycle"))
    } else if (source_lower.contains("xedit") || source_lower.contains("sseedit"))
        && (lower.contains("exception") || lower.contains("fatal") || lower.contains("error"))
        && !lower.contains("nemesis")
    {
        Some(("critical", "xedit_failure"))
    } else if source_lower.contains("nemesis")
        && lower.contains("behavior")
        && lower.contains("success")
    {
        Some(("info", "behavior_generation_success"))
    } else if (source_lower.contains("nemesis") || lower.contains("nemesis"))
        && (lower.contains("error") || lower.contains("failed"))
    {
        Some(("critical", "behavior_generation"))
    } else if lower.contains("incompatible") || lower.contains("not compatible") {
        Some(("warning", "incompatible_plugins"))
    } else if lower.contains("deleted reference") || lower.contains("udr") {
        Some(("warning", "deleted_reference"))
    } else if lower.contains("itm") || lower.contains("identical to master") {
        Some(("info", "dirty_plugin"))
    } else if lower.contains("error") || lower.contains("fatal") {
        Some(("warning", "tool_error"))
    } else if lower.contains("warning")
        && (source_lower.contains("xedit") || source_lower.contains("nemesis"))
    {
        Some(("warning", "tool_warning"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::path::PathBuf;

    #[test]
    fn diagnosis_report_collects_profile_mods_and_tool_validations() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute_batch(include_str!("../migrations/003_tool_profiles.sql"))
            .expect("create tool schema");
        conn.execute_batch(include_str!("../migrations/004_mod_dependencies.sql"))
            .expect("create dependency schema");
        conn.execute_batch(include_str!("../migrations/011_profile_plugins.sql"))
            .expect("create profile plugin schema");
        conn.execute_batch(include_str!("../migrations/006_tool_runs.sql"))
            .expect("create tool runs schema");
        conn.execute_batch(include_str!("../migrations/007_diagnostic_findings.sql"))
            .expect("create findings schema");
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
        .expect("enable mod");

        let report = build_diagnosis_report(
            &conn,
            &PathBuf::from("/tmp/slimcc-diagnosis"),
            "instance-a",
            "profile-a",
        )
        .expect("build report");

        assert_eq!(report.mods.len(), 1);
        assert_eq!(report.dependencies.len(), 1);
        assert!(report.deploy_warnings.is_empty());
        assert!(report.stored_findings.is_empty());
    }

    #[test]
    fn parse_tool_run_findings_classifies_common_tool_output() {
        let findings = parse_tool_run_findings(&[ToolRunRecord {
            id: "run".into(),
            tool_key: "xedit".into(),
            display_name: "SSEEdit".into(),
            program: "SSEEdit.exe".into(),
            arguments: vec!["-SSE".into()],
            working_directory: None,
            exit_code: Some(1),
            stdout: Some("Missing master: SkyUI_SE.esp\nIdentical to master records found".into()),
            stderr: Some("Nemesis error: failed behavior generation".into()),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: "2026-01-01T00:00:01Z".into(),
        }]);

        assert!(findings
            .iter()
            .any(|finding| finding.category == "missing_master"));
        assert!(findings
            .iter()
            .any(|finding| finding.category == "dirty_plugin"));
        assert!(findings
            .iter()
            .any(|finding| finding.category == "behavior_generation"));
        assert!(findings
            .iter()
            .any(|finding| finding.plugin.as_deref() == Some("SkyUI_SE.esp")));
    }

    #[test]
    fn parse_tool_run_findings_marks_xedit_specific_failures() {
        let findings = parse_tool_run_findings(&[ToolRunRecord {
            id: "run".into(),
            tool_key: "xedit".into(),
            display_name: "SSEEdit".into(),
            program: "SSEEdit.exe".into(),
            arguments: vec!["-SSE".into()],
            working_directory: None,
            exit_code: Some(1),
            stdout: Some("Fatal: Could not find Skyrim.esm".into()),
            stderr: None,
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: "2026-01-01T00:00:01Z".into(),
        }]);

        assert!(findings
            .iter()
            .any(|finding| finding.category == "xedit_failure"));
        assert!(findings
            .iter()
            .any(|finding| finding.severity == "critical"));
    }

    #[test]
    fn suspect_scores_prioritize_missing_dependencies_and_script_conflicts() {
        let scores = score_suspect_mods(
            &[ProfileModEntry {
                mod_id: "mod-a".into(),
                mod_name: "A".into(),
                version: None,
                source_path: PathBuf::from("/a"),
                installed_path: PathBuf::from("/a"),
                enabled: true,
                priority: 0,
                plugin_count: 0,
                rule_type: None,
                rule_target_mod_id: None,
                rule_weight: 0,
            }],
            &[ModDependencySummary {
                mod_id: "mod-a".into(),
                dependency_count: 2,
                missing_count: 1,
                status: "missing".into(),
            }],
            &[ModConflictSummary {
                mod_id: "mod-a".into(),
                overwrites_mod_ids: Vec::new(),
                overwritten_by_mod_ids: vec!["mod-b".into()],
                winning_file_count: 0,
                losing_file_count: 2,
                file_kinds: vec!["scripts".into()],
            }],
            &[],
            &[],
        );

        assert_eq!(scores[0].mod_id, "mod-a");
        assert!(scores[0].score >= 180);
    }

    #[test]
    fn resolve_finding_mod_ids_maps_plugin_names_to_owning_mods() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'SkyUI', '/mods/skyui', '/mods/skyui', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mod");
        conn.execute(
            "INSERT INTO plugins (id, mod_id, filename, plugin_type, normalized_rel_path, created_at)
             VALUES ('plugin-a', 'mod-a', 'SkyUI_SE.esp', 'esp', 'skyui_se.esp', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert plugin");

        let findings = resolve_finding_mod_ids(
            &conn,
            "instance-a",
            vec![DiagnosticFinding {
                source: "loot".into(),
                severity: "critical".into(),
                category: "missing_master".into(),
                mod_id: None,
                plugin: Some("skyui_se.esp".into()),
                message: "Missing master: SkyUI_SE.esp".into(),
                evidence: "Missing master: SkyUI_SE.esp".into(),
            }],
        )
        .expect("resolve findings");

        assert_eq!(findings[0].mod_id.as_deref(), Some("mod-a"));
    }

    #[test]
    fn store_findings_persists_log_scan_results() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/007_diagnostic_findings.sql"))
            .expect("create findings schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");

        let stored = store_findings(
            &conn,
            Some("instance-a"),
            &[DiagnosticFinding {
                source: "skse.log".into(),
                severity: "warning".into(),
                category: "script_log".into(),
                mod_id: None,
                plugin: None,
                message: "script stack".into(),
                evidence: "/logs/skse.log".into(),
            }],
        )
        .expect("store findings");

        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].category, "script_log");
        assert_eq!(stored[0].instance_id.as_deref(), Some("instance-a"));
    }

    #[test]
    fn compare_profiles_lists_added_removed_and_priority_changes() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        conn.execute_batch(include_str!("../migrations/002_mod_editor_state.sql"))
            .expect("create editor schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('base', 'instance-a', 'Stable', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('compare', 'instance-a', 'Broken', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert profiles");
        conn.execute(
            "INSERT INTO mods (id, instance_id, name, source_path, installed_path, created_at, updated_at)
             VALUES ('mod-a', 'instance-a', 'Mod A', '/mods/a', '/mods/a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('mod-b', 'instance-a', 'Mod B', '/mods/b', '/mods/b', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                    ('mod-c', 'instance-a', 'Mod C', '/mods/c', '/mods/c', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert mods");
        conn.execute(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES ('base', 'mod-a', 1, 1),
                    ('base', 'mod-b', 0, 4),
                    ('base', 'mod-c', 1, 3),
                    ('compare', 'mod-a', 1, 2),
                    ('compare', 'mod-b', 1, 4),
                    ('compare', 'mod-c', 0, 3)",
            [],
        )
        .expect("insert profile mods");

        let comparison =
            compare_profiles(&conn, "instance-a", "base", "compare").expect("compare profiles");

        assert_eq!(comparison.added_mods[0].mod_id, "mod-b");
        assert_eq!(comparison.removed_mods[0].mod_id, "mod-c");
        assert_eq!(comparison.changed_priorities[0].mod_id, "mod-a");
    }
}
