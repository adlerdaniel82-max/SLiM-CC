PRAGMA foreign_keys = ON;

ALTER TABLE nexus_mod_links ADD COLUMN nexus_file_id INTEGER;
ALTER TABLE nexus_file_cache ADD COLUMN nexus_global_file_id TEXT;
