# Codex Brief — SLiM-CC

## Project Name

SLiM-CC — Skyrim Linux Mod ControlCenter

## Mission

Build a Linux-compatible Skyrim SE/AE mod control center using Rust/Tauri and SQLite.
The app is a local desktop tool for Linux. It is safe by default and does not treat the real Skyrim `Data/` folder as the normal write target.

## Non-Negotiable Rule

**Do not write into the real Skyrim installation by default.**

Allowed in the current build:

- Read Skyrim paths
- Read `Data/`
- Read/write SLiM-CC workspace
- Build `staging/Data/`
- Generate manifests
- Launch external tools
- Manage persistent tool profiles for LOOT, SSEEdit/xEdit, and Nemesis
- Manage settings, instances, profiles, mods, conflicts, LOOT launch scaffolding, and FOMOD flows
- Store local mod dependencies and surface missing dependency warnings in summaries and deploy plans
- Import plugin-master and FOMOD file dependencies automatically
- Store optional Nexus source mappings per mod

Still intentionally guarded:

- Copy files into real `Skyrim/Data/`
- Delete files from real `Skyrim/Data/`
- Modify real plugins/loadorder files without explicit later feature gate

## Development Priority

Keep the app safe and modular while extending the already working flows:

1. Harden importer, deploy, LOOT, and FOMOD behavior
2. Expand Nexus API enrichment on top of local source mappings
3. Improve UI feedback and editor workflows
4. Validate SSEEdit/xEdit and Nemesis tool profiles against real installs
5. Add tests for safety and determinism
6. Preserve the backend as the source of truth

## Core Terms

- **Instance:** one Skyrim install + one runner/prefix configuration
- **Profile:** selected enabled mods and priorities for one instance
- **Mod:** imported folder/archive content inside SLiM-CC workspace
- **Plugin:** `.esm`, `.esp`, `.esl` file discovered inside mods
- **Staging Data:** generated merged `Data/` tree outside Skyrim install
- **Deployment Plan:** manifest of planned copy/link operations
- **DryRun:** calculate operations without writing files

## Safe First Implementation

The non-destructive deployment model remains the default:

```text
DeployTarget::DryRun
DeployTarget::Staging
DeployTarget::RealData
```

`DeployTarget::RealData` is available only through explicit guarded commands. It must create a backup first, write a manifest, and keep restore available.

## Preferred Implementation Style

- Small modules
- Explicit error types
- No hidden global state
- Use `PathBuf` for paths
- Normalize mod file paths as lowercase relative paths for conflict checks, but preserve original casing for actual file operations
- Write manifests before executing staging
- Keep UI dumb; backend owns safety logic

## Current User Story

As a Linux Skyrim modder, I want to manage instances and profiles, import mods, resolve conflicts, preview load order, and handle FOMOD packages inside SLiM-CC without risking my real Skyrim installation.

## Output Expected from Codex First

- Keep the current app compiling and consistent.
- Extend commands and UI only when they match the current architecture.
- Update docs when feature status changes.

## Avoid for Now

- Vortex collections
- OverlayFS/FUSE
- Default-enabled Real Data deployment
- TAR-style archive extraction
- MO2/Vortex collection import

Nexus API is no longer "avoid"; local source mapping is implemented and live API enrichment should be introduced before real filesystem deployment. Follow Nexus API identification/fair-use expectations and keep the app usable offline.

## Known Tool Integration Gaps

- LOOT launch now handles current LOOT Local AppData behavior through per-instance `settings.toml` and game `local_path`; preview sorting runs LOOT against an instance-local staged Data tree and real-install validation is still needed.
- SSEEdit/xEdit and Nemesis now have first-class tool profiles, but still need real-install validation and richer result handling.
- Tool profiles inherit working directory and Wine prefix from app settings unless an override is stored; xEdit argument expansion beyond `-SSE` still needs real Wine validation.
- Proton and Lutris runner execution is implemented through editable tool profiles; real-install validation is still needed.
