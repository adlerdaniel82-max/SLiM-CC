use crate::deploy;
use crate::error::SlimResult;
use crate::models::{ConflictEntry, ModConflictSummary};
use crate::models::{DeployAction, DeployTarget};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub fn list_conflicts(
    conn: &Connection,
    workspace_root: &Path,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<Vec<ConflictEntry>> {
    let plan = deploy::build_plan(
        conn,
        workspace_root,
        instance_id,
        profile_id,
        DeployTarget::DryRun,
        DeployAction::Copy,
    )?;
    Ok(plan.conflicts)
}

pub fn list_mod_conflict_summary(
    conn: &Connection,
    workspace_root: &Path,
    instance_id: &str,
    profile_id: &str,
) -> SlimResult<Vec<ModConflictSummary>> {
    let conflicts = list_conflicts(conn, workspace_root, instance_id, profile_id)?;
    Ok(summarize_mod_conflicts(&conflicts))
}

pub fn summarize_mod_conflicts(conflicts: &[ConflictEntry]) -> Vec<ModConflictSummary> {
    #[derive(Default)]
    struct Acc {
        overwrites: BTreeSet<String>,
        overwritten_by: BTreeSet<String>,
        winning_file_count: i64,
        losing_file_count: i64,
        file_kinds: BTreeSet<String>,
    }

    let mut map: BTreeMap<String, Acc> = BTreeMap::new();
    for conflict in conflicts {
        let kind = file_kind(&conflict.normalized_rel_path);
        for candidate in &conflict.candidates {
            map.entry(candidate.mod_id.clone())
                .or_default()
                .file_kinds
                .insert(kind.clone());
        }
        let losers: Vec<String> = conflict
            .candidates
            .iter()
            .filter(|candidate| candidate.mod_id != conflict.winner_mod_id)
            .map(|candidate| candidate.mod_id.clone())
            .collect();
        let winner = map.entry(conflict.winner_mod_id.clone()).or_default();
        winner.winning_file_count += 1;
        for loser in &losers {
            winner.overwrites.insert(loser.clone());
        }
        for loser in losers {
            let entry = map.entry(loser).or_default();
            entry.losing_file_count += 1;
            entry.overwritten_by.insert(conflict.winner_mod_id.clone());
        }
    }

    map.into_iter()
        .map(|(mod_id, acc)| ModConflictSummary {
            mod_id,
            overwrites_mod_ids: acc.overwrites.into_iter().collect(),
            overwritten_by_mod_ids: acc.overwritten_by.into_iter().collect(),
            winning_file_count: acc.winning_file_count,
            losing_file_count: acc.losing_file_count,
            file_kinds: acc.file_kinds.into_iter().collect(),
        })
        .collect()
}

fn file_kind(path: &str) -> String {
    let lower = path.to_lowercase();
    if lower.ends_with(".pex") || lower.starts_with("scripts/") {
        "scripts"
    } else if lower.starts_with("meshes/actors/character/animations/") || lower.contains("nemesis")
    {
        "behaviors"
    } else if lower.starts_with("meshes/") {
        "meshes"
    } else if lower.starts_with("textures/") {
        "textures"
    } else if lower.ends_with(".esp") || lower.ends_with(".esm") || lower.ends_with(".esl") {
        "plugins"
    } else {
        "other"
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ConflictCandidate;
    use std::path::PathBuf;

    #[test]
    fn summarize_mod_conflicts_reports_overwrites_and_file_kinds() {
        let summaries = summarize_mod_conflicts(&[ConflictEntry {
            normalized_rel_path: "scripts/foo.pex".into(),
            winner_mod_id: "winner".into(),
            candidates: vec![
                ConflictCandidate {
                    mod_id: "winner".into(),
                    priority: 20,
                    original_rel_path: "scripts/foo.pex".into(),
                    abs_source_path: PathBuf::from("/winner/scripts/foo.pex"),
                },
                ConflictCandidate {
                    mod_id: "loser".into(),
                    priority: 10,
                    original_rel_path: "scripts/foo.pex".into(),
                    abs_source_path: PathBuf::from("/loser/scripts/foo.pex"),
                },
            ],
        }]);

        let winner = summaries
            .iter()
            .find(|entry| entry.mod_id == "winner")
            .unwrap();
        let loser = summaries
            .iter()
            .find(|entry| entry.mod_id == "loser")
            .unwrap();

        assert_eq!(winner.overwrites_mod_ids, vec!["loser"]);
        assert_eq!(winner.winning_file_count, 1);
        assert_eq!(loser.overwritten_by_mod_ids, vec!["winner"]);
        assert_eq!(loser.losing_file_count, 1);
        assert_eq!(winner.file_kinds, vec!["scripts"]);
    }
}
