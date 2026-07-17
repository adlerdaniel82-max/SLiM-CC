PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS mod_dependencies (
    id TEXT PRIMARY KEY,
    mod_id TEXT NOT NULL,
    dependency_type TEXT NOT NULL,
    target_mod_id TEXT,
    target_value TEXT NOT NULL,
    notes TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE,
    FOREIGN KEY(target_mod_id) REFERENCES mods(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_mod_dependencies_mod_id ON mod_dependencies(mod_id);
CREATE INDEX IF NOT EXISTS idx_mod_dependencies_target_mod_id ON mod_dependencies(target_mod_id);
