use crate::db;
use crate::error::SlimResult;
use crate::models::WorkspaceInfo;
use crate::paths;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub workspace_root: PathBuf,
    pub database_path: PathBuf,
    pub db: Mutex<rusqlite::Connection>,
    pub vfs: Mutex<crate::vfs::VfsManager>,
}

impl AppState {
    pub fn initialize() -> SlimResult<Self> {
        let workspace_root = paths::workspace_root()?;
        paths::ensure_workspace(&workspace_root)?;
        let database_path = paths::database_path(&workspace_root);
        let db = db::open_database(&database_path)?;

        Ok(Self {
            workspace_root,
            database_path,
            db: Mutex::new(db),
            vfs: Mutex::new(crate::vfs::VfsManager::new()),
        })
    }

    pub fn workspace_info(&self) -> WorkspaceInfo {
        WorkspaceInfo {
            workspace_root: self.workspace_root.clone(),
            database_path: self.database_path.clone(),
        }
    }

    pub fn with_connection<T, F>(&self, f: F) -> SlimResult<T>
    where
        F: FnOnce(&Connection) -> SlimResult<T>,
    {
        let conn = self
            .db
            .lock()
            .map_err(|_| crate::error::SlimError::Safety("database lock poisoned".into()))?;
        f(&conn)
    }

    pub fn with_connection_mut<T, F>(&self, f: F) -> SlimResult<T>
    where
        F: FnOnce(&mut Connection) -> SlimResult<T>,
    {
        let mut conn = self
            .db
            .lock()
            .map_err(|_| crate::error::SlimError::Safety("database lock poisoned".into()))?;
        f(&mut conn)
    }
}
