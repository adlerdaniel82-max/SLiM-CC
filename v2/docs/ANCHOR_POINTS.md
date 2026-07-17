# Codex Anchor Points

Use these as stable points when editing the project.

## Safety Anchors

- `DeployTarget::RealData` must remain explicit, confirmed, backed up, and non-default.
- All writes must go through path guards in `paths.rs`.
- No service may directly write to real Skyrim paths outside guarded deploy/restore commands.

## Architecture Anchors

- Tauri commands are thin wrappers.
- Business logic belongs in service modules.
- Database access should be isolated in repository-style functions.
- Deployment is two-step:
  1. build plan
  2. execute plan
- Importers may attach installer state, but the backend owns the state shape.
- External tools should use `tool_profiles` rather than one-off frontend state.

## Naming Anchors

- Project: `SLiM-CC`
- Workspace folder: `slim-cc`
- Staged output: `staging/Data`
- Manifest: `last_deploy_plan.json`

## Token-Saving Instruction for Future Codex Sessions

When continuing this project, start from these files only unless needed:

1. `docs/CODEX_BRIEF.md`
2. `docs/ANCHOR_POINTS.md`
3. `docs/SAFETY_MODEL.md`
4. `src-tauri/src/models.rs`
5. `src-tauri/src/deploy.rs`
6. `src-tauri/src/staging.rs`
7. `src-tauri/migrations/`
8. `docs/COMMANDS_SPEC.md`

Do not reread all docs unless architecture changes are requested.
