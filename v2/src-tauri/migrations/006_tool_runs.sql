PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS tool_runs (
    id TEXT PRIMARY KEY,
    tool_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    program TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    working_directory TEXT,
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tool_runs_tool_key_finished
ON tool_runs(tool_key, finished_at DESC);
