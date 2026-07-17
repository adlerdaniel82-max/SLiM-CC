PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS diagnostic_findings (
    id TEXT PRIMARY KEY,
    instance_id TEXT,
    source TEXT NOT NULL,
    severity TEXT NOT NULL,
    category TEXT NOT NULL,
    mod_id TEXT,
    plugin TEXT,
    message TEXT NOT NULL,
    evidence TEXT NOT NULL,
    found_at TEXT NOT NULL,
    FOREIGN KEY(instance_id) REFERENCES instances(id) ON DELETE CASCADE,
    FOREIGN KEY(mod_id) REFERENCES mods(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_diagnostic_findings_instance
ON diagnostic_findings(instance_id, found_at DESC);

CREATE INDEX IF NOT EXISTS idx_diagnostic_findings_mod
ON diagnostic_findings(mod_id, found_at DESC);
