# Profile Plugin Activation Design

## Goal

Replace the duplicated deploy conflict summary with a persistent per-profile plugin activation table. Users can disable individual `.esm`, `.esp`, or `.esl` files that belong to an enabled mod, and disabled plugins stay out of DryRun, Staging, Real Deploy, LOOT seed files, and LOOT preview for that profile.

## Scope

- Only SLiM-CC files under `/home/webuser/web/schnueddels.de/private/SLiM-CC` and the browser preview mirror under `/home/webuser/web/schnueddels.de/public_html/slimcc` are in scope.
- Backend remains the source of truth.
- The real Skyrim `Data/` folder remains guarded and non-default.
- Existing mod-level profile enablement remains unchanged.

## Data Model

Add a `profile_plugins` table keyed by `(profile_id, plugin_id)`.

- Missing row means enabled.
- Stored row with `enabled = 0` means disabled for that profile.
- Foreign keys cascade with deleted profiles or plugins.

The table stores only plugin state, not load-order metadata. Mod priority stays in `profile_mods`.

## Backend Behavior

Add models and commands for:

- `list_profile_plugins(instance_id, profile_id) -> Vec<ProfilePluginEntry>`
- `update_profile_plugins(UpdateProfilePluginsRequest) -> Vec<ProfilePluginEntry>`

Each entry includes plugin id, mod id/name, filename, plugin type, normalized relative path, mod enabled state, effective plugin enabled state, priority, and local dependency status for that owning mod.

Deploy plan generation must skip disabled plugin files while still deploying other enabled mod files. A file is skipped only when its normalized path maps to a disabled plugin for the active profile.

LOOT seed files must include only enabled plugins from enabled mods. LOOT import may update mod priority and mod enabled state, but it must not re-enable plugins that the user explicitly disabled.

## UI Behavior

The Deploy tab gets a Downloads-style plugin table:

- checkbox/action column
- plugin filename
- type
- owning mod
- status/details

The existing `Show conflicts` and `Mod conflict summary` buttons are removed from Deploy to avoid duplicating the Mods/Diagnostics conflict views. Conflict details still remain available in Mods and Diagnostics.

Changing a checkbox immediately persists through the backend and refreshes deploy/load-order data. The Activity panel shows command output like other actions.

## Documentation

Update:

- `README.md`
- `TODO_FOR_CODEX.md`
- `docs/COMMANDS_SPEC.md`
- `docs/DATA_MODEL.md`

Mention that profile-level plugin activation exists and deploy/LOOT flows honor it.

## Testing

Backend tests must prove:

- disabled plugins are omitted from deploy plans while non-plugin files from the same mod still deploy
- LOOT seed/plugin filename queries omit disabled plugins
- update/list round trip persists profile plugin state

Frontend build and Rust tests must pass before completion is claimed.
