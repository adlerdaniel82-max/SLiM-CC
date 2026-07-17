# Architecture

## High-Level Flow

```text
Frontend
  ↓ Tauri commands
Rust backend
  ↓
SQLite repositories
  ↓
Domain services
  ├── instance registry
  ├── profile manager
  ├── mod scanner
  ├── conflict resolver
  ├── deployment planner
  ├── staging executor
  └── tool runner
```

## Rust Module Layout

```text
src-tauri/src/
├── main.rs
├── app_state.rs
├── error.rs
├── db.rs
├── models.rs
├── commands.rs
├── paths.rs
├── instance.rs
├── profiles.rs
├── mods.rs
├── nexus.rs
├── scanner.rs
├── conflicts.rs
├── diagnostics.rs
├── deploy.rs
├── staging.rs
├── fomod.rs
├── settings.rs
└── tools.rs
```

## Module Responsibilities

### `paths.rs`

- Resolve SLiM-CC data directory.
- Resolve instance workspace paths.
- Guard write paths.

### `db.rs`

- Open SQLite.
- Apply schema/migrations.
- Provide connection helper.

### `models.rs`

- Shared structs used by commands and services.

### `instance.rs`

- Create/list/update game instances.
- Validate paths.

### `profiles.rs`

- Create/list profiles.
- Enable/disable mods per profile.
- Set priorities.

### `mods.rs`

- Import/register mod folders.
- Store mod metadata.
- Store installer state for supported package types.
- Persist editor metadata such as tags, notes, and simple rules.
- Store and evaluate local dependency entries for mods, plugins, files, and manual requirements.
- Import plugin-master and FOMOD file dependencies into local dependency entries.

### `nexus.rs`

- Store optional Nexus source mappings per imported mod.
- Expose cached Nexus requirement metadata.
- Keep live API fetching separate from local dependency evaluation.

### `scanner.rs`

- Recursively scan mod files.
- Detect Data root.
- Detect plugin files.

### `conflicts.rs`

- Group files by normalized relative path.
- Select winning file by priority.
- Return conflict report.
- Build per-mod conflict summaries for diagnostics.

### `diagnostics.rs`

- Build deterministic JSON diagnosis reports.
- Combine profile mods, dependency summaries, conflict summaries, deploy warnings, tool validation output, recent tool-run output, parsed findings, and suspect-mod scores.
- Scan selected log folders for non-mutating diagnostic findings.

### `deploy.rs`

- Build `DeployPlan`.
- Never execute directly.
- Add advisory warnings for enabled mods with missing local dependencies.

### `staging.rs`

- Execute staging plan safely.
- Path guard required before every write.

### `tools.rs`

- Build and launch native/Wine/Proton/Lutris external tool commands.
- Capture command output for UI diagnostics.
- Persist recent tool-profile and LOOT execution output for diagnosis reports.
- Store and launch persistent tool profiles for LOOT, SSEEdit/xEdit, and Nemesis.
- Validate configured executable path, effective working directory, and Wine prefix without launching the tool.
- Proton and Lutris runner variants are implemented as editable tool-profile runners.

### `settings.rs`

- Store app-level defaults and external tool paths.
- Currently persists install path, derived-or-external Data path, Wine prefix, and LOOT executable path.
- Tool profiles can override working directory and Wine prefix, but otherwise inherit those defaults from app settings.

### `fomod.rs`

- Parse FOMOD package metadata.
- Build selection previews.
- Resolve selected files for installation.
- Persist installer state through the mods layer.

## Frontend Scope

The frontend is now a structured desktop UI rather than a minimal bootstrap screen.

Current UI areas:

- Overview with activity/log panel
- Settings for workspace defaults and tool paths
- Instances editor
- Profiles editor
- Mods editor
- Deploy/conflict workflow

Not yet first-class UI areas:

- Nexus live API sync and update metadata
- Tool result parsing beyond raw stdout/stderr

The frontend should remain thin: state and validation belong in the backend, and the UI should stay focused on presentation and command invocation.
