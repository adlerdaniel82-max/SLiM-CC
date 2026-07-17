# Safety Model

## Core Safety Rule

SLiM-CC must be non-destructive by default.

The real Skyrim install is read-only by default. Real deployment is an explicit guarded action with confirmation, backup creation, manifest writing, and restore support.

## Allowed Write Locations

- SLiM-CC workspace
- SQLite database
- `instances/<id>/mods/`
- `instances/<id>/profiles/`
- `instances/<id>/staging/`
- `instances/<id>/logs/`
- `instances/<id>/backups/`
- installer-state records inside SQLite
- Real `Skyrim/Data/` only through explicit `RealData` deploy or restore commands

## Forbidden Write Locations By Default

- Real Skyrim executable directory
- Real Steam game files
- Real Wine prefix game files, except external tool logs if tools create them themselves

## Deployment Targets

```rust
pub enum DeployTarget {
    DryRun,
    Staging,
    RealData,
}
```

Current behavior:

- `DryRun`: calculate only
- `Staging`: write only under SLiM-CC staging path
- `RealData`: explicit UI command only; creates a pre-deploy backup and rolls back if execution fails

## Manifest First

Before any staging or real execution, write a manifest:

```text
instances/<instance>/profiles/<profile>/last_deploy_plan.json
```

The manifest must contain:

- profile id
- target
- timestamp
- list of operations
- source path
- target path
- owning mod
- conflict status

## Path Guard

Every file write must pass a guard:

```text
canonical(target).starts_with(canonical(instance_workspace))
```

For staging:

```text
canonical(target).starts_with(canonical(instance_workspace/staging))
```

Never trust frontend-provided paths for write operations.

## Backup And Restore

Real deployment and restore require:

1. Backup manifest
2. User confirmation
3. Reversible snapshot contents
4. Restore command
5. Pre-restore backup before replacing the active Data folder

Snapshot restore checks that the snapshot source root matches the active instance Data path before replacing that Data folder.

## Current Open Safety Work

- Add more automated coverage around path guards and staging writes.
- Keep the real `Data/` deployment gate explicit and non-default.
- Make import and installer edge cases fail closed rather than guessing.
