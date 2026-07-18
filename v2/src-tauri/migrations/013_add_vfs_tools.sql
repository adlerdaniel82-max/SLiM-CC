INSERT INTO tool_profiles (
    id, tool_key, display_name, runner_type, arguments_json, enabled, created_at, updated_at
)
VALUES
    ('tool-fnis', 'fnis', 'FNIS', 'Wine', '[]', 0, datetime('now'), datetime('now')),
    ('tool-pandora', 'pandora', 'Pandora Behaviour Engine', 'Wine', '[]', 0, datetime('now'), datetime('now')),
    ('tool-bodyslide', 'bodyslide', 'BodySlide & Outfit Studio', 'Wine', '[]', 0, datetime('now'), datetime('now'))
ON CONFLICT(tool_key) DO NOTHING;
