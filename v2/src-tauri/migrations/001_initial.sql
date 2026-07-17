PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS instances (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    game_type TEXT NOT NULL DEFAULT 'skyrimse',
    install_path TEXT NOT NULL,
    data_path TEXT NOT NULL,
    game_starter_path TEXT,
    runner_type TEXT NOT NULL DEFAULT 'manual',
    wine_prefix TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS profiles (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(instance_id) REFERENCES instances(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS mods (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    name TEXT NOT NULL,
    version TEXT,
    source_path TEXT NOT NULL,
    installed_path TEXT NOT NULL,
    enabled_default INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(instance_id) REFERENCES instances(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS profile_mods (
    profile_id TEXT NOT NULL,
    mod_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(profile_id, mod_id),
    FOREIGN KEY(profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS mod_files (
    id TEXT PRIMARY KEY,
    mod_id TEXT NOT NULL,
    original_rel_path TEXT NOT NULL,
    normalized_rel_path TEXT NOT NULL,
    abs_source_path TEXT NOT NULL,
    file_size INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_mod_files_mod_id ON mod_files(mod_id);
CREATE INDEX IF NOT EXISTS idx_mod_files_norm_path ON mod_files(normalized_rel_path);

CREATE TABLE IF NOT EXISTS plugins (
    id TEXT PRIMARY KEY,
    mod_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    plugin_type TEXT NOT NULL,
    normalized_rel_path TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS mod_installers (
    mod_id TEXT PRIMARY KEY,
    installer_type TEXT NOT NULL,
    state_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tools (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    name TEXT NOT NULL,
    executable_path TEXT NOT NULL,
    arguments TEXT,
    working_directory TEXT,
    runner_type TEXT NOT NULL DEFAULT 'wine',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(instance_id) REFERENCES instances(id) ON DELETE CASCADE
);
