PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS nexus_mod_links (
    mod_id TEXT PRIMARY KEY,
    game_domain TEXT NOT NULL,
    nexus_mod_id INTEGER NOT NULL,
    source_url TEXT NOT NULL DEFAULT '',
    last_checked_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_nexus_mod_links_game_mod
ON nexus_mod_links(game_domain, nexus_mod_id);

CREATE TABLE IF NOT EXISTS nexus_requirements (
    id TEXT PRIMARY KEY,
    mod_id TEXT NOT NULL,
    required_game_domain TEXT NOT NULL,
    required_nexus_mod_id INTEGER,
    required_name TEXT NOT NULL,
    requirement_type TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'nexus',
    notes TEXT NOT NULL DEFAULT '',
    fetched_at TEXT NOT NULL,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_nexus_requirements_mod_id
ON nexus_requirements(mod_id);
