use crate::error::SlimResult;
use crate::instance;
use crate::models::{CreateProfileRequest, Profile, UpdateProfileRequest};
use crate::paths;
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub fn create_profile(conn: &mut Connection, request: CreateProfileRequest) -> SlimResult<Profile> {
    let instance_id = request.instance_id.clone();
    let name = request.name.clone();
    instance::assert_instance_exists(conn, &instance_id)?;
    let tx = conn.transaction()?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    tx.execute(
        "INSERT INTO profiles (id, instance_id, name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, &instance_id, &name, &now, &now],
    )?;

    let mut stmt =
        tx.prepare("SELECT id FROM mods WHERE instance_id = ?1 ORDER BY created_at ASC")?;
    let mod_rows = stmt.query_map(params![&instance_id], |row| row.get::<_, String>(0))?;
    let mut mod_ids = Vec::new();
    for row in mod_rows {
        mod_ids.push(row?);
    }
    drop(stmt);

    for mod_id in mod_ids {
        tx.execute(
            "INSERT OR IGNORE INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES (?1, ?2, 0, 0)",
            params![&id, mod_id],
        )?;
    }

    tx.commit()?;

    Ok(Profile {
        id,
        instance_id,
        name,
    })
}

pub fn update_profile(conn: &mut Connection, request: UpdateProfileRequest) -> SlimResult<Profile> {
    let id = request.id.clone();
    let instance_id = request.instance_id.clone();
    let name = request.name.clone();
    instance::assert_instance_exists(conn, &instance_id)?;
    let tx = conn.transaction()?;
    let now = Utc::now().to_rfc3339();

    let updated = tx.execute(
        "UPDATE profiles
         SET instance_id = ?2,
             name = ?3,
             updated_at = ?4
         WHERE id = ?1",
        params![&id, &instance_id, &name, &now],
    )?;

    if updated == 0 {
        return Err(crate::error::SlimError::NotFound(format!(
            "profile not found: {id}"
        )));
    }

    tx.execute(
        "DELETE FROM profile_mods WHERE profile_id = ?1",
        params![&id],
    )?;

    let mut stmt =
        tx.prepare("SELECT id FROM mods WHERE instance_id = ?1 ORDER BY created_at ASC")?;
    let mod_rows = stmt.query_map(params![&instance_id], |row| row.get::<_, String>(0))?;
    let mut mod_ids = Vec::new();
    for row in mod_rows {
        mod_ids.push(row?);
    }
    drop(stmt);

    for mod_id in mod_ids {
        tx.execute(
            "INSERT OR IGNORE INTO profile_mods (profile_id, mod_id, enabled, priority)
             VALUES (?1, ?2, 0, 0)",
            params![&id, mod_id],
        )?;
    }

    tx.commit()?;

    Ok(Profile {
        id,
        instance_id,
        name,
    })
}

pub fn delete_profile(
    conn: &mut Connection,
    workspace_root: &std::path::Path,
    profile_id: &str,
) -> SlimResult<()> {
    let tx = conn.transaction()?;
    let instance_id: String = tx.query_row(
        "SELECT instance_id FROM profiles WHERE id = ?1",
        params![profile_id],
        |row| row.get(0),
    )?;

    let deleted = tx.execute("DELETE FROM profiles WHERE id = ?1", params![profile_id])?;
    if deleted == 0 {
        return Err(crate::error::SlimError::NotFound(format!(
            "profile not found: {profile_id}"
        )));
    }

    let profile_dir = paths::profile_path(workspace_root, &instance_id, profile_id);
    if profile_dir.exists() {
        std::fs::remove_dir_all(&profile_dir)?;
    }
    let staging_data_path = paths::staging_data_path(workspace_root, &instance_id);
    if staging_data_path.exists() {
        std::fs::remove_dir_all(&staging_data_path)?;
    }

    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_workspace(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("slimcc-{name}-{}", Uuid::new_v4()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn delete_profile_removes_profile_directory_and_instance_staging() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))
            .expect("create schema");
        let workspace_root = temp_workspace("delete-profile-staging");
        let profile_dir = paths::profile_path(&workspace_root, "instance-a", "profile-a");
        let staging_data_path = paths::staging_data_path(&workspace_root, "instance-a");
        fs::create_dir_all(&profile_dir).expect("create profile dir");
        fs::create_dir_all(&staging_data_path).expect("create staging dir");
        fs::write(profile_dir.join("last_deploy_plan.json"), "{}").expect("write profile file");
        fs::write(staging_data_path.join("Plugin.esp"), "plugin").expect("write staged file");
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

        delete_profile(&mut conn, &workspace_root, "profile-a").expect("delete profile");

        assert!(!profile_dir.exists());
        assert!(!staging_data_path.exists());

        let _ = fs::remove_dir_all(workspace_root);
    }
}
