use crate::error::{SlimError, SlimResult};
use crate::models::{CreateInstanceRequest, GameInstance, UpdateInstanceRequest};
use crate::paths;
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub fn create_instance(
    conn: &Connection,
    workspace_root: &std::path::Path,
    request: CreateInstanceRequest,
) -> SlimResult<GameInstance> {
    let name = request.name.clone();
    let install_path = request.install_path.clone();
    let data_path = request.data_path.clone();
    let game_starter_path = request.game_starter_path.clone();
    let runner_type = request.runner_type.clone();
    let wine_prefix = request.wine_prefix.clone();
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let instance_root = paths::instance_root(workspace_root, &id);
    std::fs::create_dir_all(paths::mods_root(workspace_root, &id))?;
    std::fs::create_dir_all(paths::profiles_root(workspace_root, &id))?;
    std::fs::create_dir_all(paths::staging_data_path(workspace_root, &id))?;
    std::fs::create_dir_all(paths::logs_root(workspace_root, &id))?;
    std::fs::create_dir_all(paths::backups_root(workspace_root, &id))?;
    std::fs::create_dir_all(&instance_root)?;

    conn.execute(
        "INSERT INTO instances (id, name, game_type, install_path, data_path, game_starter_path, runner_type, wine_prefix, created_at, updated_at)
         VALUES (?1, ?2, 'skyrimse', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            &id,
            &name,
            install_path.to_string_lossy().to_string(),
            data_path.to_string_lossy().to_string(),
            game_starter_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            &runner_type,
            wine_prefix.as_ref().map(|p| p.to_string_lossy().to_string()),
            &now,
            &now
        ],
    )?;

    Ok(GameInstance {
        id,
        name,
        game_type: "skyrimse".into(),
        install_path,
        data_path,
        game_starter_path,
        runner_type,
        wine_prefix,
    })
}

pub fn list_instances(conn: &Connection) -> SlimResult<Vec<GameInstance>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, game_type, install_path, data_path, game_starter_path, runner_type, wine_prefix
         FROM instances
         ORDER BY created_at DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        let instance = GameInstance {
            id: row.get(0)?,
            name: row.get(1)?,
            game_type: row.get(2)?,
            install_path: std::path::PathBuf::from(row.get::<_, String>(3)?),
            data_path: std::path::PathBuf::from(row.get::<_, String>(4)?),
            game_starter_path: row
                .get::<_, Option<String>>(5)?
                .map(std::path::PathBuf::from),
            runner_type: row.get(6)?,
            wine_prefix: row
                .get::<_, Option<String>>(7)?
                .map(std::path::PathBuf::from),
        };
        let _ = crate::workspace::instance_label(&instance);
        Ok(instance)
    })?;

    let mut instances = Vec::new();
    for row in rows {
        instances.push(row?);
    }
    Ok(instances)
}

pub fn update_instance(
    conn: &Connection,
    request: UpdateInstanceRequest,
) -> SlimResult<GameInstance> {
    let name = request.name.clone();
    let install_path = request.install_path.clone();
    let data_path = request.data_path.clone();
    let game_starter_path = request.game_starter_path.clone();
    let runner_type = request.runner_type.clone();
    let wine_prefix = request.wine_prefix.clone();
    let now = Utc::now().to_rfc3339();

    let updated = conn.execute(
        "UPDATE instances
         SET name = ?2,
             install_path = ?3,
             data_path = ?4,
             game_starter_path = ?5,
             runner_type = ?6,
             wine_prefix = ?7,
             updated_at = ?8
         WHERE id = ?1",
        params![
            &request.id,
            &name,
            install_path.to_string_lossy().to_string(),
            data_path.to_string_lossy().to_string(),
            game_starter_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            &runner_type,
            wine_prefix
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            &now
        ],
    )?;

    if updated == 0 {
        return Err(SlimError::NotFound(format!(
            "instance not found: {}",
            request.id
        )));
    }

    Ok(GameInstance {
        id: request.id,
        name,
        game_type: "skyrimse".into(),
        install_path,
        data_path,
        game_starter_path,
        runner_type,
        wine_prefix,
    })
}

pub fn delete_instance(
    conn: &mut Connection,
    workspace_root: &std::path::Path,
    instance_id: &str,
) -> SlimResult<()> {
    let tx = conn.transaction()?;
    let deleted = tx.execute("DELETE FROM instances WHERE id = ?1", params![instance_id])?;
    if deleted == 0 {
        return Err(SlimError::NotFound(format!(
            "instance not found: {instance_id}"
        )));
    }

    let instance_root = paths::instance_root(workspace_root, instance_id);
    if instance_root.exists() {
        std::fs::remove_dir_all(&instance_root)?;
    }

    tx.commit()?;
    Ok(())
}

pub fn get_instance_by_id(conn: &Connection, instance_id: &str) -> SlimResult<GameInstance> {
    conn.query_row(
        "SELECT id, name, game_type, install_path, data_path, game_starter_path, runner_type, wine_prefix
         FROM instances
         WHERE id = ?1",
        params![instance_id],
        |row| {
            Ok(GameInstance {
                id: row.get(0)?,
                name: row.get(1)?,
                game_type: row.get(2)?,
                install_path: std::path::PathBuf::from(row.get::<_, String>(3)?),
                data_path: std::path::PathBuf::from(row.get::<_, String>(4)?),
                game_starter_path: row
                    .get::<_, Option<String>>(5)?
                    .map(std::path::PathBuf::from),
                runner_type: row.get(6)?,
                wine_prefix: row
                    .get::<_, Option<String>>(7)?
                    .map(std::path::PathBuf::from),
            })
        },
    )
    .map_err(Into::into)
}

pub fn assert_instance_exists(conn: &Connection, instance_id: &str) -> SlimResult<()> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(1) FROM instances WHERE id = ?1",
        params![instance_id],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(SlimError::NotFound(format!(
            "instance not found: {instance_id}"
        )));
    }
    Ok(())
}
