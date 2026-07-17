# Tauri Command Spec

## Current Commands

### `workspace_info() -> WorkspaceInfo`

Returns workspace and database path.

### `create_instance(request: CreateInstanceRequest) -> GameInstance`

Creates safe instance workspace and stores Skyrim path metadata.

### `update_instance(request: UpdateInstanceRequest) -> GameInstance`

Updates an existing instance.

### `list_instances() -> Vec<GameInstance>`

Implemented.

### `list_mods(instance_id: String) -> Vec<Mod>`

Implemented.

### `update_mod(request: UpdateModRequest) -> Mod`

Implemented. Updates mod name, enabled-default state, tags, notes, and rule metadata.

### `list_mod_dependencies(mod_id: String) -> Vec<ModDependencyStatus>`

Returns local dependency entries for one mod with evaluated `satisfied`/`missing` status.

### `update_mod_dependencies(request: UpdateModDependenciesRequest) -> Vec<ModDependencyStatus>`

Replaces a mod's local dependency entries and returns the evaluated dependency status list.

### `list_instance_dependency_summary(instance_id: String) -> Vec<ModDependencySummary>`

Returns per-mod dependency counts and missing counts for the selected instance.

### `list_nexus_mod_links(instance_id: String) -> Vec<NexusModLink>`

Returns optional Nexus source mappings for mods in one instance.

### `upsert_nexus_mod_link(request: UpdateNexusModLinkRequest) -> Option<NexusModLink>`

Creates, updates, or clears a mod's Nexus source mapping.

### `parse_nexus_source_link(source_url: String) -> Option<NexusSourceLink>`

Parses supported Nexus source links for UI autofill. Supported inputs are NXM ModManager links such as `nxm://skyrimspecialedition/mods/179647/files/750608?...` and Nexus page links such as `https://www.nexusmods.com/skyrimspecialedition/mods/179647?tab=files&file_id=750608`. The returned `sanitized_url` keeps only stable source identifiers and drops temporary download parameters such as `key`, `expires`, `user_id`, or `md5`. Final CDN download URLs are not treated as reliable metadata sources.

The desktop shell also registers `nxm://` as a Tauri deep-link scheme. Links opened from the browser are routed through the same parser; if a mod is selected, its Nexus mapping is updated directly, otherwise the parsed link is held as pending input for the Mod editor.

### `list_nexus_requirements(mod_id: String) -> Vec<NexusRequirement>`

Returns cached Nexus requirement metadata for a mod.

### `list_nexus_requirement_status(mod_id: String) -> Vec<NexusRequirementStatus>`

Returns cached Nexus requirements for a mod plus local fulfillment status. Matching currently checks other mods in the same instance by stored Nexus game/mod ID first, then by exact mod name. This command is the shared basis for the selected-mod Nexus panel and later diagnosis/deploy warnings.

### `list_nexus_mod_cache(instance_id: String) -> Vec<NexusModCache>`

Returns cached Nexus API metadata for mapped mods in one instance, including Nexus name, version, update time, endorsements, downloads, and fetch timestamp.

### `validate_nexus_api() -> NexusApiStatus`

Reads the Nexus API key saved in Settings, calls a harmless Nexus v3 metadata endpoint, and returns API/rate-limit status without exposing the full key.

### `sync_nexus_mod(mod_id: String) -> NexusSyncResult`

Uses the stored Nexus mapping for a mod to fetch live Nexus v3 metadata from `https://api.nexusmods.com/v3`. Mod metadata is fetched from `GET /games/{game_domain}/mods/{game_scoped_id}`. If a Nexus File ID is configured, file metadata is fetched from `GET /games/{game_domain}/mod-files/{game_scoped_id}` and materialized dependencies from `GET /mod-files/{id}/dependencies/materialized`.

### `create_profile(request: CreateProfileRequest) -> Profile`

Implemented.

### `update_profile(request: UpdateProfileRequest) -> Profile`

Implemented.

### `list_profiles(instance_id: String) -> Vec<Profile>`

Implemented.

### `import_mod_folder(request: ImportModFolderRequest) -> ImportedModReport`

Imports a mod from a folder or from a supported archive. ZIP/FOMOD/OMOD and 7z archives are extracted into the workspace first, then scanned or processed as FOMOD packages. FOMOD dependencyType/defaultType rules are evaluated against local Data/plugin evidence so unavailable compatibility patches can remain unselected.

### `scan_mod_files(request: ScanModFilesRequest) -> Vec<ModFile>`

Implemented.

### `list_profile_mods(instance_id: String, profile_id: String) -> Vec<ProfileMod>`

Implemented.

### `update_profile_mods(request: UpdateProfileModsRequest) -> ()`

Implemented.

### `list_profile_plugins(instance_id: String, profile_id: String) -> Vec<ProfilePluginEntry>`

Returns effective plugin activation rows for enabled mods in the selected profile. Missing `profile_plugins` rows mean the plugin is enabled. Rows include plugin filename/type, owning mod, priority, and local dependency status for the owning mod.

### `update_profile_plugins(request: UpdateProfilePluginsRequest) -> Vec<ProfilePluginEntry>`

Persists per-profile plugin activation. Enabling a plugin removes the explicit override; disabling writes `enabled = 0`. Disabled plugins are omitted from deploy plans and LOOT seed files while other non-plugin files from the same enabled mod still participate.

### `build_dry_run_plan(instance_id, profile_id) -> DeployPlan`

Calculates operations without writing files.
Implemented. The returned plan includes advisory dependency warnings for enabled mods with missing local requirements, excludes profile-disabled plugins, and warns/skips impossible file-vs-directory target collisions before staging writes.

### `execute_staging_plan(instance_id, profile_id) -> DeployPlan`

Builds staged `Data/` safely under workspace.
Implemented. The returned plan includes the same advisory dependency and path-collision warnings as dry-run planning.

### `list_conflicts(instance_id, profile_id) -> Vec<ConflictEntry>`

Implemented convenience wrapper around dry-run plan.

### `list_mod_conflict_summary(instance_id, profile_id) -> Vec<ModConflictSummary>`

Returns per-mod conflict summaries with overwrite/overwritten-by relationships, winning/losing file counts, and broad file kinds.

### `build_diagnosis_report(instance_id, profile_id) -> DiagnosisReport`

Builds deterministic JSON evidence for diagnostics. Current report includes profile mods, dependency summaries, mod conflict summaries, deploy warnings, tool validation results, parsed tool-output findings, persisted imported findings, recent tool runs, and suspect-mod scores.

### `import_diagnostic_logs(log_dir: String, instance_id: Option<String>) -> Vec<StoredDiagnosticFinding>`

Scans `.log` and `.txt` files in a selected folder for crash, script, fatal, and error hints. Findings are stored in `diagnostic_findings`, optionally mapped back to owning mods by referenced plugin name, and returned for the Diagnose tab.

### `compare_profiles(instance_id, base_profile_id, compare_profile_id) -> ProfileComparison`

Compares effective enabled state and priorities between a known-good/reference profile and the current comparison profile. Returns added mods, removed mods, and priority differences for diagnosis workflows.

### `preview_fomod_package(package_root: String) -> FomodPackagePreview`

Returns parsed FOMOD package metadata for a package on disk.

### `preview_mod_fomod(mod_id: String) -> FomodPackagePreview`

Returns parsed FOMOD metadata and saved installer state for an imported mod.

### `launch_tool(request: ToolLaunchRequest) -> CommandPreview`

Launches an external tool and returns the resolved command preview plus captured execution output.

### `list_tool_profiles() -> Vec<ToolProfile>`

Returns persistent tool profiles for LOOT, SSEEdit/xEdit, Nemesis, and user-configured tools.

### `upsert_tool_profile(request: UpdateToolProfileRequest) -> ToolProfile`

Creates or updates a persistent tool profile.

### `launch_tool_profile(request: LaunchToolProfileRequest) -> CommandPreview`

Launches a configured tool profile and returns command preview plus captured stdout/stderr metadata.

### `validate_tool_profiles() -> Vec<ToolProfileValidation>`

Checks configured tool profiles without launching them. Reports whether each executable, effective working directory, and Wine prefix exists.

### `list_recent_tool_runs() -> Vec<ToolRunRecord>`

Returns recent persisted tool execution records with resolved command, exit code, stdout/stderr, and timestamps.

### `preview_loot_sort(instance_id: String, profile_id: String) -> LootSortPreview`

Builds a staged preview `Data` tree, launches LOOT against an instance-local preview game path, reads the generated `loadorder.txt`/`plugins.txt`, and returns current versus proposed profile order without mutating saved profile priority. Profile-disabled plugins are not seeded and are ignored when syncing plugin-to-mod ownership.

### `launch_loot(request: LaunchLootRequest) -> LootLaunchResult`

Launches LOOT using the configured `loot` tool profile when available, including runner type and effective Wine prefix, with fallback to the older app setting. It writes per-instance LOOT `settings.toml` with game-specific `local_path` and returns captured stdout/stderr metadata. Profile sync imports generated `plugins.txt`/`loadorder.txt` from that local path, supports both star-prefixed and plain active plugin lists, and does not use profile-disabled plugins to re-enable mods.

### `pick_directory(title: Option<String>, default_path: Option<String>) -> Option<String>`

Native folder picker for the UI.

### `pick_file(title: Option<String>, default_path: Option<String>) -> Option<String>`

Native file picker for the UI.

### `get_app_settings() -> AppSettings`

Reads persisted application defaults, including Skyrim paths, tool defaults, LOOT executable path, and Mod Download Ordner.

### `update_app_settings(request: UpdateAppSettingsRequest) -> AppSettings`

Writes persisted application defaults.

### `list_mod_download_candidates() -> Vec<ModDownloadCandidate>`

Lists folders and known archives from the configured Mod Download Ordner. ZIP/FOMOD/OMOD/7z entries are importable; unsupported archive types are reported as non-importable candidates.
