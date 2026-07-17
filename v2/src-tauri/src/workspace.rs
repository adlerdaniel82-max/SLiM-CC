use crate::error::SlimResult;
use crate::models::{GameInstance, ModRecord, Profile};
use rusqlite::Connection;
use serde_json;

pub fn list_profiles(conn: &Connection, instance_id: &str) -> SlimResult<Vec<Profile>> {
    let mut stmt = conn.prepare(
        "SELECT id, instance_id, name
         FROM profiles
         WHERE instance_id = ?1
         ORDER BY created_at DESC",
    )?;

    let rows = stmt.query_map([instance_id], |row| {
        Ok(Profile {
            id: row.get(0)?,
            instance_id: row.get(1)?,
            name: row.get(2)?,
        })
    })?;

    let mut profiles = Vec::new();
    for row in rows {
        profiles.push(row?);
    }
    Ok(profiles)
}

pub fn list_mods(conn: &Connection, instance_id: &str) -> SlimResult<Vec<ModRecord>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.instance_id, m.name, m.version,
                COALESCE((SELECT SUM(mf.file_size) FROM mod_files mf WHERE mf.mod_id = m.id), 0),
                m.source_path, m.installed_path, m.enabled_default,
                COALESCE(mes.tags_json, '[]'),
                COALESCE(mes.notes, ''),
                COALESCE(mes.rule_type, 'none'),
                mes.rule_target_mod_id,
                COALESCE(mes.rule_weight, 0)
         FROM mods m
         LEFT JOIN mod_editor_state mes ON mes.mod_id = m.id
         WHERE m.instance_id = ?1
         ORDER BY m.created_at DESC",
    )?;

    let rows = stmt.query_map([instance_id], |row| {
        let tags_json: String = row.get(8)?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        Ok(ModRecord {
            id: row.get(0)?,
            instance_id: row.get(1)?,
            name: row.get(2)?,
            version: row.get(3)?,
            size_bytes: row.get(4)?,
            source_path: std::path::PathBuf::from(row.get::<_, String>(5)?),
            installed_path: std::path::PathBuf::from(row.get::<_, String>(6)?),
            enabled_default: row.get::<_, i64>(7)? != 0,
            tags,
            notes: {
                let notes: String = row.get(9)?;
                if notes.is_empty() {
                    None
                } else {
                    Some(notes)
                }
            },
            rule_type: {
                let rule_type: String = row.get(10)?;
                if rule_type == "none" {
                    None
                } else {
                    Some(rule_type)
                }
            },
            rule_target_mod_id: row.get(11)?,
            rule_weight: row.get(12)?,
        })
    })?;

    let mut mods = Vec::new();
    for row in rows {
        mods.push(row?);
    }
    Ok(mods)
}

pub fn get_mod_by_id(conn: &Connection, mod_id: &str) -> SlimResult<ModRecord> {
    conn.query_row(
        "SELECT m.id, m.instance_id, m.name, m.version,
                COALESCE((SELECT SUM(mf.file_size) FROM mod_files mf WHERE mf.mod_id = m.id), 0),
                m.source_path, m.installed_path, m.enabled_default,
                COALESCE(mes.tags_json, '[]'),
                COALESCE(mes.notes, ''),
                COALESCE(mes.rule_type, 'none'),
                mes.rule_target_mod_id,
                COALESCE(mes.rule_weight, 0)
         FROM mods m
         LEFT JOIN mod_editor_state mes ON mes.mod_id = m.id
         WHERE m.id = ?1",
        [mod_id],
        |row| {
            let tags_json: String = row.get(8)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok(ModRecord {
                id: row.get(0)?,
                instance_id: row.get(1)?,
                name: row.get(2)?,
                version: row.get(3)?,
                size_bytes: row.get(4)?,
                source_path: std::path::PathBuf::from(row.get::<_, String>(5)?),
                installed_path: std::path::PathBuf::from(row.get::<_, String>(6)?),
                enabled_default: row.get::<_, i64>(7)? != 0,
                tags,
                notes: {
                    let notes: String = row.get(9)?;
                    if notes.is_empty() {
                        None
                    } else {
                        Some(notes)
                    }
                },
                rule_type: {
                    let rule_type: String = row.get(10)?;
                    if rule_type == "none" {
                        None
                    } else {
                        Some(rule_type)
                    }
                },
                rule_target_mod_id: row.get(11)?,
                rule_weight: row.get(12)?,
            })
        },
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INITIAL_SCHEMA: &str = include_str!("../migrations/001_initial.sql");

    #[test]
    fn list_profiles_orders_by_profile_creation_without_mod_alias() {
        let conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(INITIAL_SCHEMA).expect("create schema");
        conn.execute(
            "INSERT INTO instances (id, name, game_type, install_path, data_path, runner_type, created_at, updated_at)
             VALUES ('instance-a', 'Skyrim', 'skyrimse', '/game', '/game/Data', 'manual', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert instance");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-old', 'instance-a', 'Old', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert old profile");
        conn.execute(
            "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
             VALUES ('profile-new', 'instance-a', 'New', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z')",
            [],
        )
        .expect("insert new profile");

        let profiles = list_profiles(&conn, "instance-a").expect("list profiles");

        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].id, "profile-new");
        assert_eq!(profiles[1].id, "profile-old");
    }
}

pub fn instance_label(instance: &GameInstance) -> String {
    format!("{} ({})", instance.name, instance.id)
}
