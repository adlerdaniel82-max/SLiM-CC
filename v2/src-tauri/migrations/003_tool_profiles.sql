PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS tool_profiles (
    id TEXT PRIMARY KEY,
    tool_key TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    executable_path TEXT,
    runner_type TEXT NOT NULL DEFAULT 'Wine',
    arguments_json TEXT NOT NULL DEFAULT '[]',
    working_directory TEXT,
    wine_prefix TEXT,
    log_path TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO tool_profiles (
    id,
    tool_key,
    display_name,
    runner_type,
    arguments_json,
    enabled,
    created_at,
    updated_at
)
VALUES
    ('tool-loot', 'loot', 'LOOT', 'Native', '[]', 1, datetime('now'), datetime('now')),
    ('tool-xedit', 'xedit', 'SSEEdit / xEdit', 'Wine', '["-SSE"]', 1, datetime('now'), datetime('now')),
    ('tool-nemesis', 'nemesis', 'Nemesis', 'Wine', '[]', 1, datetime('now'), datetime('now'))
ON CONFLICT(tool_key) DO NOTHING;
