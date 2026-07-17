PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS mod_editor_state (
    mod_id TEXT PRIMARY KEY,
    tags_json TEXT NOT NULL DEFAULT '[]',
    notes TEXT NOT NULL DEFAULT '',
    rule_type TEXT NOT NULL DEFAULT 'none',
    rule_target_mod_id TEXT,
    rule_weight INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);
