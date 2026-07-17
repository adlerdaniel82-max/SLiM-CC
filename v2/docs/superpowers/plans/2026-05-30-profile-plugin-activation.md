# Profile Plugin Activation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add persistent per-profile plugin activation and show it as a Downloads-style table in Deploy.

**Architecture:** Store plugin activation in SQLite as profile-owned state. Backend commands expose effective plugin rows and all deploy/LOOT paths filter through that state. The frontend renders and updates the state, but does not derive authoritative activation rules.

**Tech Stack:** Rust/Tauri, rusqlite, TypeScript, Vite, SQLite migrations.

---

### Task 1: Persist Profile Plugin State

**Files:**
- Create: `src-tauri/migrations/011_profile_plugins.sql`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/mods.rs`

- [ ] Write Rust tests in `src-tauri/src/mods.rs` for list/update profile plugin state.
- [ ] Add migration with `profile_plugins(profile_id, plugin_id, enabled, updated_at)`.
- [ ] Add `ProfilePluginEntry`, `UpdateProfilePluginEntry`, and `UpdateProfilePluginsRequest`.
- [ ] Implement `list_profile_plugins` and `update_profile_plugins`.
- [ ] Run `cargo test profile_plugin --lib` and confirm the new tests pass.

### Task 2: Honor Disabled Plugins in Planning and LOOT

**Files:**
- Modify: `src-tauri/src/deploy.rs`
- Modify: `src-tauri/src/mods.rs`

- [ ] Write failing deploy test: disabling `Patch.esp` omits that operation but keeps `meshes/item.nif` from the same mod.
- [ ] Write failing LOOT filename test: disabled plugins are omitted from active and loadorder seed lists.
- [ ] Filter deploy candidates by disabled plugin normalized paths.
- [ ] Filter `profile_plugin_filenames` by profile plugin state.
- [ ] Keep LOOT import from re-enabling explicitly disabled plugins.
- [ ] Run targeted Rust tests.

### Task 3: Expose Commands to Tauri

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] Add command wrappers for list/update profile plugins.
- [ ] Register commands in `tauri::generate_handler!`.
- [ ] Run `cargo check --lib`.

### Task 4: Render Deploy Plugin Table

**Files:**
- Modify: `index.html`
- Modify: `src/main.ts`
- Modify: `src/i18n.ts`
- Modify: `src/style.css`

- [ ] Add Deploy table mount point.
- [ ] Add TypeScript state/type for profile plugins.
- [ ] Refresh plugin rows with active profile data.
- [ ] Render a Downloads-style plugin table with enable/disable action.
- [ ] Persist checkbox changes via `update_profile_plugins`.
- [ ] Remove Deploy conflict-summary duplication from UI buttons and listeners.
- [ ] Run `npm run build`.

### Task 5: Docs and Preview Mirror

**Files:**
- Modify: `README.md`
- Modify: `TODO_FOR_CODEX.md`
- Modify: `docs/COMMANDS_SPEC.md`
- Modify: `docs/DATA_MODEL.md`
- Modify: `public_html/slimcc/*` through the existing build/release path if available.

- [ ] Update docs for persistent profile plugin activation.
- [ ] Build the frontend.
- [ ] Copy/update the preview mirror if the project’s release script performs it without touching other projects.
- [ ] Run final verification commands and report exact results.

## Self-Review

- Spec coverage: data model, backend filtering, LOOT behavior, UI replacement, docs, and verification are covered.
- Placeholder scan: no placeholder tasks remain.
- Type consistency: request/entry names match the planned backend and frontend API.
