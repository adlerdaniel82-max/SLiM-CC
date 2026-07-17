# SLiM-CC Roadmap

## Phase 0 — Repository Bootstrap

Goal: create a compilable Tauri/Rust app skeleton.

Acceptance criteria:

- App starts.
- SQLite database can be created/opened.
- Frontend can call at least one backend command.
- Workspace path can be resolved.

## Phase 1 — Safe Instance Registry

Goal: register Skyrim installations without modifying them.

Features:

- Add instance manually.
- Store game path, derived-or-external Data path, runner type, prefix path.
- Validate whether paths exist.
- Diagnose SKSE, Skyrim executable, Data folder, plugins folder.

Acceptance criteria:

- User can save an instance.
- App can show green/yellow/red diagnostics.
- No write into Skyrim folder.

## Phase 2 — Mod Import and Indexing

Goal: import already-unpacked mod folders.

Features:

- Copy or register mod folder into SLiM-CC workspace.
- Detect Data root inside mod folder.
- Index all files.
- Detect plugins: `.esm`, `.esp`, `.esl`.

Acceptance criteria:

- Imported mod appears in database.
- Files are stored in `mod_files`.
- Plugins are stored in `plugins`.

## Phase 3 — Profiles and Priorities

Goal: enable/disable mods per profile.

Features:

- Create profile.
- Enable mods for profile.
- Set priority integer.
- Higher priority wins file conflicts.

Acceptance criteria:

- Same mod can be enabled in one profile and disabled in another.
- Conflict winner is deterministic.

## Phase 4 — Deployment Plan and Conflict View

Goal: calculate staged Data output without writing.

Features:

- Build deployment plan.
- Detect file conflicts by relative path.
- Pick winning mod by priority.
- Produce JSON manifest.

Acceptance criteria:

- DryRun returns operations.
- Conflicts are visible.
- Manifest can be saved to workspace.
- Per-mod conflict summaries show which mods overwrite or are overwritten, grouped by file kind.

## Phase 5 — Staging Executor

Goal: build `staging/Data/` safely.

Features:

- Clear previous staging output only inside SLiM-CC workspace.
- Create directories.
- Copy or symlink files from mods to staging.
- Write manifest before execution.

Acceptance criteria:

- `staging/Data/` is generated.
- Real Skyrim `Data/` remains unchanged.
- Errors abort safely.

## Phase 6 — External Tool Runner

Goal: launch tools in correct native/Wine/Proton/Lutris environment.

Current status: implemented, pending real-install validation. Native, Wine, Proton, and Lutris command execution work through the generic tool runner, and persistent profiles exist for LOOT, SSEEdit/xEdit, and Nemesis.

Features:

- Register tool path.
- Store args and working directory.
- Runner abstraction:
  - Wine
  - Proton
  - Lutris
  - Native
- Dry-run command preview.
- Capture stdout/stderr for diagnostics.

Acceptance criteria:

- App can show command preview.
- App can start a configured executable.
- App can report captured output from a tool run.
- App can save first-class tool profiles.
- Tool profiles use global working-directory/Wine-prefix defaults unless explicit overrides are stored.
- App can validate configured executable, effective working directory, and Wine prefix before launch.
- App persists recent tool-run output for diagnostics.
- App can parse common tool output categories into deterministic diagnostic findings.
- LOOT app launch honors the configured Tool profile runner; Nemesis defaults to the executable folder when no working-directory override is stored.
- Proton uses `SLIMCC_PROTON_BIN` or `proton` with `proton run`; Lutris uses `SLIMCC_LUTRIS_BIN` or `lutris` with `--exec`.

## Phase 7 — Loadorder Integration

Goal: keep load order visible and controlled in the app.

Features:

- Profile-local load order editing
- LOOT launch scaffold and profile sync code for LOOT output files
- LOOT-backed preview against an instance-local staged Data tree
- Persisted profile mod order and enable state

Acceptance criteria:

- User can inspect and update load order from the Mods view.
- LOOT launch uses the configured executable path.
- LOOT Local AppData handling writes per-instance `settings.toml` with game `local_path`; sync still needs real-install validation.
- LOOT output is captured and available to the diagnosis report.
- LOOT preview returns a proposed order without mutating the saved profile order.
- LOOT profile sync imports `loadorder.txt`/`plugins.txt` with zero-based profile priorities and supports plain as well as star-prefixed active plugin lines.

## Phase 8 — Installer and Mod Workflow Hardening

Goal: make existing import and installer flows more robust.

Current status: partially implemented. Local dependency entries can be edited per mod, plugin masters and FOMOD file dependencies are imported automatically, dependency status is shown as an instance summary, and missing requirements are surfaced as advisory deploy-plan warnings.

Features:

- FOMOD edge cases and dependency rules
- Import/reconfigure parity
- Better installer state handling
- Better import and scan feedback in the UI
- Mod editor metadata for tags, notes, and simple rules
- Local dependency status for mods, plugins, files, and manual checks

Acceptance criteria:

- Existing FOMOD packages can be reconfigured.
- Import and reconfigure stay on the same code path.
- Mod metadata persists in the workspace and appears in the editor.
- Missing local dependencies are visible before staging.
- Imported plugin masters and FOMOD file dependencies become local dependency entries.
- Imported log findings can be persisted, filtered by severity, and mapped back to owning mods where referenced plugins are known.
- Known-good and current profiles can be compared for added, removed, and reordered mods.

## Phase 9 — Optional Later Features

- Nexus API metadata
- Nexus live API requirement/update sync on top of stored source mappings
- Broader Nexus download/source matching and collection import on top of the implemented NXM handler
- Better plugin parser edge-case coverage
- Broader FOMOD rule transfer into local dependency status
- Profile-bisect helpers and richer diagnosis remediation actions
- Real-world parser tuning for LOOT, SSEEdit/xEdit, Nemesis, SKSE, crash logs, and Papyrus logs
- Optional local AI analysis provider over that diagnosis report
- Default-enabled Real Data deploy after live verification
- OverlayFS/FUSE experiment
- Real-install validation for SSEEdit/xEdit
- Real-install validation for Nemesis
- Proton/Lutris real-install validation
- Tool log parsing beyond raw stdout/stderr capture
