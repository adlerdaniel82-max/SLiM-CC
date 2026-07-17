# TODO for Codex

Stand: 2026-05-15

This file tracks the implementation status for SLiM-CC. The target is a fully functional Linux mod manager. Real writes into the Skyrim installation are explicit guarded actions only; the default workflow stays DryRun/Staging until tool integration, backup, rollback, and tests are complete enough.

## Implemented

- Tauri v2 app shell with Rust backend and TypeScript frontend.
- Sidebar navigation with dedicated views for:
  - `Overview`
  - `Settings`
  - `Instances`
  - `Profiles`
  - `Mods`
  - `Deploy`
- Always-visible activity/log panel for debugging during early development.
- UI help tooltips for buttons and input fields, including path-specific hints for Skyrim install, Data folder, Wine prefix, tool working directories, logs, and mod sources.
- Topbar navigation with separate `Downloads` view for batch imports, Nexus links, and collection analysis.
- SQLite-backed app settings.
- Path pickers for folders/files via native dialogs.
- Instance management:
  - list instances
  - create instance
  - update instance
- Profile management:
  - list profiles
  - create profile
  - update profile
  - validate instance ownership before creating/updating profiles
- Mod management:
  - import mod folders
  - scan mod files
  - edit imported mods
  - mod tags, notes, and simple rule metadata
  - local dependency editor for Mod/Plugin/File/Manual requirements
  - automatic plugin-master dependency import from `.esm/.esp/.esl` TES4 `MAST` subrecords
  - persistent per-profile activation for individual `.esm/.esp/.esl` plugin files
  - automatic FOMOD `fileDependency` import from ModuleConfig dependency groups
  - instance-level dependency summary and missing dependency display
  - profile rule metadata now contributes to deploy warnings and suspect scoring for obvious force-enable/force-disable conflicts
  - Nexus v3 source mapping fields per mod: game domain, Nexus mod id, optional Nexus file id, source URL
  - NXM ModManager and Nexus page-link parser that fills game/mod/file fields and strips temporary download query data
  - Downloads view for the download-folder candidate list, Nexus-link parsing, and collection analysis entry points
  - desktop `nxm://` deep-link registration through Tauri and single-instance forwarding into the running app
  - Nexus API key validation from Settings with masked display; the full key is never printed in output
  - live Nexus v3 metadata/file/dependency sync for mapped mods with SQLite cache
  - Nexus status panel for selected mods with sync state, version hints, and requirement fulfilled/missing status
  - list profile mods
  - update profile mod state and priority
- Conflict and deployment flow:
  - dry-run plan generation
  - staging execution
  - pre-deploy backup snapshots and restore path for real filesystem deployment
  - Deploy UI commands for real deploy, backup listing, and restore
  - restore creates a pre-restore backup and verifies snapshot source path before replacing the active Data folder
  - conflict listing
  - per-mod conflict summary showing overwrites, overwritten-by, winning/losing file counts, and file kinds
  - deploy-plan warnings for enabled mods with missing local dependencies
  - Deploy view plugin table for disabling a plugin file without disabling the entire owning mod
  - deploy planner detects file-vs-directory target collisions and skips impossible nested writes with a warning before staging runs
- Tool integration:
  - generic tool launcher command with native, Wine, Proton, and Lutris execution
  - persistent tool profiles for LOOT, SSEEdit/xEdit, and Nemesis
  - built-in default tool profiles are seeded automatically and remain editable
  - Tools UI for executable path, runner type, arguments, optional working-directory/Wine-prefix overrides, log path, enabled state, and test launch
  - Settings UI derives the normal `Data` path from the Skyrim install path unless an external Data folder is enabled
  - LOOT executable configuration
  - LOOT launch scaffold with stdout/stderr capture, configured Tool-profile runner support, and profile sync code for generated load-order files
  - current LOOT Local AppData handling through per-instance `settings.toml` and game `local_path`
  - LOOT import handles both star-prefixed and plain `plugins.txt` active plugin lists
  - LOOT preview builds a staged Data tree, starts LOOT against an instance-local preview path, reads generated `loadorder.txt`/`plugins.txt`, and does not mutate profile order
  - tool profile validation report for executable, effective working directory, and Wine prefix checks
  - SSEEdit/xEdit launches now default to `-SSE` when the profile does not supply an explicit game switch
  - standard tool log path defaults to `logfile-<tool>.log` in the effective working directory when no explicit log path is configured
- Diagnostics:
  - structured diagnosis JSON report containing profile mods, dependency summary, mod conflict summary, deploy warnings, and tool validation results
  - Diagnose tab with readable panels for missing dependencies, suspect mods, tool status, deploy/tool/log warnings, mod conflicts, and recent tool runs
  - persisted recent tool-run records for profile launches and LOOT launches
  - basic parser for common LOOT/SSEEdit/Nemesis-style output categories
  - basic log folder scan for `.log`/`.txt` crash, script, and error hints with persisted diagnostic findings
  - plugin-to-mod mapping for parsed findings where the referenced plugin is known locally
  - deterministic suspect-mod scoring from missing dependencies, deploy warnings, high-risk file conflicts, and mapped diagnostic findings
  - tool-specific diagnostics now classify obvious xEdit failures and feed profile-rule conflicts into the suspect ranking
  - Diagnose UI severity filter and clickable suspect mods
  - profile comparison between a known-good/reference profile and the current profile
  - UI actions for tool validation, diagnosis report JSON, log import, profile comparison, and mod conflict summary
- FOMOD support:
  - package detection
  - wizard flow in the UI
  - installer state persistence
  - reconfigure support for existing FOMOD mods
  - dependencyType/defaultType handling for unavailable compatibility patches, so missing armor/mod/plugin requirements no longer force patch installation
- Archive/download support:
  - configurable Mod Download Ordner
  - download candidates are offered in the Mods view
  - ZIP/FOMOD/OMOD/7z/RAR archives are extracted into the workspace before import
- UI compacting:
  - globally smaller buttons and fields
  - narrower sidebar and tighter panel spacing
  - compact Mods/Downloads table layout retained
  - less relevant tool options moved behind an Advanced disclosure
  - browser preview mirror updated under `public_html/slimcc/`

## Next Step To Implement

Complete and validate the first-class external tool workflow before moving further toward real filesystem deployment.

- Validate LOOT launch and preview against a real current LOOT install:
  - confirm per-instance `settings.toml` and game `local_path` are honored
  - confirm `plugins.txt`/`loadorder.txt` are imported from the path LOOT actually uses with real LOOT output
  - confirm the preview staging tree matches what LOOT expects without mutating profile order
  - capture and display failure output from real LOOT runs clearly
- Validate SSEEdit/xEdit launch support against a real install:
  - default Skyrim SE argument `-SSE`
  - decide whether additional xEdit path/INI/plugin-list arguments are needed for Wine launches
  - runner-aware launch through native/Wine/Proton/Lutris
  - captured output and clear UI errors
- Validate Nemesis launch support against a real install:
  - runner-aware launch through native/Wine/Proton/Lutris
  - configured working directory; default is now the executable parent folder when no override is set
  - captured output and clear UI errors
- Expand the local dependency model further:
  - improve plugin master parsing for edge cases beyond standard TES4 `MAST` subrecords
  - expand FOMOD dependency transfer beyond file dependencies if needed
  - expand rule semantics beyond obvious force-enable/force-disable conflicts into planner/load-order helpers
  - improve missing file/plugin indicators with clearer remediation hints
- Extend Nexus support from local source mapping and NXM handling to richer optional live metadata:
  - user-controlled OAuth/SSO flow to replace the current local key-file bootstrap
  - broader Nexus-hosted mod requirements using v3 file-level dependency endpoints
  - update comparison UI from cached Nexus version/update timestamps
  - download relationships and file selection flows
  - import-time source matching so downloaded archives can be pre-associated with Nexus metadata when enough evidence exists
- Expand the local diagnostics foundation for later AI assistance:
  - improve parsers for persisted LOOT output, SSEEdit/xEdit output, Nemesis output, SKSE/crash/Papyrus logs
  - tune plugin/log finding ownership mapping against real tool and crash-log formats
  - keep the report deterministic JSON; AI must consume evidence, not invent state
  - add profile-bisect helpers on top of the implemented profile comparison
  - prepare a provider-neutral AI settings model for local endpoints such as Ollama, llama.cpp server, LM Studio/OpenAI-compatible local APIs, or LocalAI

## High Priority After Tool Profiles

Add Nexus API support early enough to make dependency and update checks useful.

- Store Nexus metadata separately from local mod records.
- Map imported mods to Nexus game/mod/file IDs when the user provides or confirms the source. Basic manual mapping, NXM/Nexus URL parsing, selected-mod status panel, local Nexus requirement matching, and first live sync are implemented.
- Cache mod requirements when exposed by the API response, version/update information, file metadata, endorsements/download metadata, and source URLs. Remaining work: broader import-time source detection and richer update remediation UI.
- Use Nexus API metadata as an optional enrichment layer, not as the only dependency source.
- Keep the app usable offline with local dependency checks:
  - plugin master requirements
  - FOMOD file dependency decisions
  - local file/plugin existence checks (manual entries implemented)
  - user-defined rules (stored, planner evaluation still open)
- Add clear UI states for:
  - unknown dependency source
  - locally satisfied dependency
  - Nexus-known dependency
  - missing dependency
  - possible update available

## Later Diagnostic AI Assistance

Goal: help with large 300-400 mod profiles by ranking likely causes of instability without allowing AI to mutate load order or files directly.

- First build deterministic evidence:
  - conflict/dependency graph across mods, plugins, files, and profiles
  - LOOT warnings and synced output
  - SSEEdit/xEdit diagnostics and cleaning/override warnings
  - Nemesis output and behavior-generation status
  - SKSE, crash log, and Papyrus log parsing where available
  - Nexus/API/source metadata as optional enrichment
- Then add a local AI analysis provider:
  - disabled by default
  - Ollama
  - llama.cpp server
  - LM Studio or other OpenAI-compatible local endpoint
  - LocalAI/custom HTTP endpoint
- AI output should be advisory only:
  - likely culprit ranking
  - explanation tied to concrete evidence
  - suggested test order
  - suggested profile-bisect groups
  - no direct file changes, deploy, or load-order mutation without explicit deterministic app commands

## Needs Completion Before Filesystem Deployment

These items should be complete before SLiM-CC writes into the real Skyrim `Data/` folder.

- Real deployment into the live Skyrim `Data/` folder now has a backup/snapshot path and an OS-adapted link strategy, but still needs final live verification before default enablement.
- DryRun and Staging manifests must be complete and reviewable.
- Staging execution must be deterministic across repeated runs.
- Conflict resolution must be stable and visible in the UI.
- Mod rule metadata must be evaluated or clearly marked as informational only.
- Profile-level plugin activation is persisted and honored by DryRun/Staging/Real Deploy/LOOT preview; broader remediation hints for disabled plugins remain future polish.
- Dependency checks are available locally for manual requirements, imported plugin masters, and imported FOMOD file dependencies; Nexus API enrichment is still needed.
- LOOT launch/sync must be verified against current LOOT releases on a real install.
- SSEEdit/xEdit and Nemesis configured tool profiles must be validated against real installs.
- Tool validation report exists; real-tool launch behavior still needs validation on an actual install.
- UI-level error handling and status feedback must stay visible for import, scan, deploy, FOMOD, LOOT, SSEEdit/xEdit, Nemesis, and generic tool launch flows.
- Backup and rollback metadata are implemented for deploy snapshots, but still need a final live verification pass.
- Real Data deployment must require explicit confirmation and must never be the default target.

## Needs Test Coverage Before Filesystem Deployment

- Better automated tests for:
  - importer behavior
  - conflict resolution determinism
  - staging path guards
  - FOMOD selection/install flows
  - LOOT sync behavior
  - dependency summary and deploy warning behavior
  - diagnosis report completeness
  - mod conflict summary grouping
  - persisted tool-run capture
  - diagnostic parser categories and suspect scoring
  - tool launch output capture
  - path handling for Wine prefixes and tool working directories
  - backup/rollback plan generation before real deployment is enabled

## Additional Open Work

- More complete FOMOD handling for remaining edge cases and complex dependency graphs.
- Better dependency remediation UI and automatic matching to Nexus metadata.
- Structured diagnosis report and optional local AI-assisted analysis over that report.
- Validate the LOOT preview implementation against real LOOT releases and adjust arguments/settings if current LOOT expects a different preview workflow.
- More complete mod/editor workflows for advanced rules and load-order editing helpers.
- Editor follow-up work for rule evaluation and relation-aware helpers.
- Add a user-facing tool library so arbitrary extra tools can be defined while presets stay available for LOOT, SSEEdit/xEdit, and Nemesis.
- Validate Proton and Lutris runners with real installed tools.
- Add TAR-style archive support after the ZIP/7z/RAR path has been validated with real Nexus downloads.
- Add Nexus collection import only after the local manager path is reliable.

## Notes for Future Work

- Keep repository logic in the Rust backend; do not move authoritative state into the frontend.
- Keep the default workflow non-destructive. DryRun and Staging stay the normal path.
- When adding a new feature, update this file so the state stays explicit.

## Historical Bootstrap Tasks

The following items were part of the initial bootstrap phase and are now done:

- Verify Tauri v2 config and dependency versions.
- Split database write/read helpers from `commands.rs` into repository functions.
- Add missing modules: `instance.rs`, `profiles.rs`, `mods.rs`, `conflicts.rs` if needed.
- Implement command: `list_instances`.
- Implement command: `create_profile`.
- Implement command: `import_mod_folder`.
- Implement command: `scan_mod_files`.
- Implement command: `list_conflicts`.

## Old "Do Not Implement Yet" Items

These are no longer blocked in the same way as before:

- LOOT
- FOMOD

Still intentionally deferred:

- SSEEdit/xEdit real-install validation
- Nemesis real-install validation
- Vortex collections
- OverlayFS/FUSE
- default-enabled real Data deployment
