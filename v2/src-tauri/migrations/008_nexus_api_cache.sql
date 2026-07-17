PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS nexus_mod_cache (
    mod_id TEXT PRIMARY KEY,
    game_domain TEXT NOT NULL,
    nexus_mod_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '',
    updated_time TEXT,
    endorsement_count INTEGER,
    mod_downloads INTEGER,
    fetched_at TEXT NOT NULL,
    raw_json TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_nexus_mod_cache_game_mod
ON nexus_mod_cache(game_domain, nexus_mod_id);

CREATE TABLE IF NOT EXISTS nexus_file_cache (
    id TEXT PRIMARY KEY,
    mod_id TEXT NOT NULL,
    nexus_file_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '',
    category_name TEXT NOT NULL DEFAULT '',
    is_primary INTEGER NOT NULL DEFAULT 0,
    uploaded_time TEXT,
    mod_version TEXT NOT NULL DEFAULT '',
    file_name TEXT NOT NULL DEFAULT '',
    size_in_bytes INTEGER,
    fetched_at TEXT NOT NULL,
    raw_json TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_nexus_file_cache_mod_id
ON nexus_file_cache(mod_id);
