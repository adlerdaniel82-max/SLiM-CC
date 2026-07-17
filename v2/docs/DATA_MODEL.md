# SQLite Data Model

## Main Tables

See `src-tauri/migrations/` for the schema files applied by `db.rs`.

Current core tables:

- `instances`
- `app_settings`
- `profiles`
- `mods`
- `profile_mods`
- `profile_plugins`
- `mod_files`
- `plugins`
- `mod_installers`
- `mod_editor_state`
- `mod_dependencies`
- `nexus_mod_links`
- `nexus_requirements`
- `tool_runs`
- `tool_profiles`
- `tools`

## Important Rules

- Store original relative file paths for file operations.
- Store normalized relative paths for conflict detection.
- Do not store absolute target paths for real Skyrim deployment in MVP.
- Keep profile state separate from mod import state.
- Keep installer state separate from base mod metadata.

## Conflict Winner Rule

For one `normalized_rel_path`:

1. Only enabled mods in active profile participate.
2. Highest `priority` wins.
3. If priority ties, newest profile_mod row or mod name sort may be used, but this should be explicit.
4. Result must be deterministic.

Recommended deterministic tie-break:

```text
ORDER BY priority DESC, mod_id DESC
```

## Plugin Detection

A plugin is any file under a mod Data tree ending with:

- `.esm`
- `.esp`
- `.esl`

Case-insensitive.

## Profile Plugin Activation

`profile_plugins` stores profile-specific overrides for individual plugin files:

- `profile_id`
- `plugin_id`
- `enabled`
- `updated_at`

No row means the plugin is enabled for that profile. A row with `enabled = 0` disables only that `.esm/.esp/.esl` file. The owning mod can remain enabled, so non-plugin files from that mod still participate in deploy plans.

DryRun, Staging, Real Deploy, LOOT seed files, and LOOT preview ignore disabled plugins. This state is per profile and cascades when profiles or plugins are deleted.

## Installer State

`mod_installers` stores package-specific installer state as JSON.

Current use:

- FOMOD package selection state
- Reconfigure metadata for imported mods

The importer keeps this state attached to the mod record so the same package can be reopened later.

## Mod Editor State

`mod_editor_state` stores lightweight editor metadata per mod:

- tags as JSON list
- freeform notes
- rule type
- rule target mod id
- rule weight

This state is separate from import/scanner output so editor changes can evolve independently of the file index.

## Mod Dependencies

`mod_dependencies` stores local requirements owned by a mod:

- dependency type: `mod`, `plugin`, `file`, or `manual`
- optional target mod id for direct mod-to-mod requirements
- target value for mod name, plugin filename, normalized relative file path, or manual note
- notes and timestamps

The backend evaluates these entries against local SQLite evidence. Mod requirements can resolve by target mod id or matching mod name in the same instance. Plugin requirements resolve against `plugins.filename`; file requirements resolve against `mod_files.normalized_rel_path`. Manual requirements remain visible but unresolved until a richer rule system exists.

Automatic local sources:

- Plugin masters are read from TES4 `MAST` subrecords in `.esm/.esp/.esl` files and stored as plugin dependencies.
- FOMOD `fileDependency` entries from dependency groups are stored as file dependencies.
- FOMOD dependencyType/defaultType rules are evaluated during preview/import against local file/plugin evidence. Options that resolve to `NotUsable` stay unavailable, which lets compatibility patch groups with missing requirements be skipped.

Deploy plans include warnings for enabled mods whose local dependencies are missing. This is still advisory; it does not block DryRun or Staging yet.

## Nexus Metadata

`nexus_mod_links` stores optional source mapping per imported mod:

- local `mod_id`
- Nexus game domain, for example `skyrimspecialedition`
- Nexus mod id
- source URL
- last checked timestamp

`nexus_requirements` stores cached Nexus requirement metadata when the live API response exposes requirements in a supported shape. Nexus metadata enriches local checks rather than replacing them.

`NexusRequirementStatus` is derived at query time rather than stored. It marks cached Nexus requirements as fulfilled when another mod in the same instance is mapped to the required Nexus game/mod ID, with exact-name matching as a fallback for requirements that do not carry a Nexus mod id.

`nexus_mod_cache` stores the latest live Nexus metadata for mapped mods:

- Nexus name and version
- update timestamp
- endorsements and download count
- raw API JSON for later parser improvements
- fetch timestamp

`nexus_file_cache` stores live Nexus v3 file metadata for mapped mods when a Nexus File ID is configured:

- Nexus file id
- Nexus v3 global file id
- displayed file name and archive file name
- category and primary-file flag
- mod version and upload timestamp
- size and raw API JSON

## Tool Profiles

`tool_profiles` stores first-class external tools:

- `loot`
- `xedit`
- `nemesis`

Each profile stores executable path, runner type, argument list JSON, optional working-directory and Wine-prefix overrides, optional log path, and enabled state. If a profile leaves working directory or Wine prefix empty, the launcher falls back to the global app settings. The older `tools` table remains unused legacy scaffolding until it is removed or migrated.

## Tool Runs

`tool_runs` stores recent external-tool execution output:

- tool key and display name
- resolved program and arguments
- effective working directory
- exit code
- stdout/stderr
- start and finish timestamps

The Diagnose tab and `DiagnosisReport` use these records as deterministic evidence for later analysis. Current persistence covers tool-profile launches and LOOT launches.

## Diagnostic Findings

`diagnostic_findings` stores imported log/tool evidence that should survive UI refreshes:

- source file/tool
- severity and category
- optional instance
- optional owning mod id
- optional referenced plugin
- message, evidence path/text, and timestamp

The importer maps plugin names back to locally indexed mods when possible. `DiagnosisReport` combines current parsed tool output with persisted findings for suspect-mod scoring and later deterministic AI input.

## Download Candidates

The global settings can store a Mod Download Ordner. SLiM-CC lists direct child folders and known archive files from that folder as `ModDownloadCandidate` values for the UI. ZIP/FOMOD/OMOD, 7z, and RAR candidates are importable and are extracted into the workspace before scanning; TAR-style candidates remain future work.
