# SLiM-CC v2 Architecture

The authoritative release number is maintained in [`VERSION.md`](VERSION.md).

## Frontend

The frontend is split into small modules:

- `src/app.ts`: workflow coordination and event delegation
- `src/core/`: backend bridge, store, DOM helpers and menu behavior
- `src/features/`: feature-specific rules such as FOMOD selection semantics
- `src/ui/`: stateless shell and dialog rendering
- `src/styles/`: compact desktop presentation

No feature depends on a global list of required DOM nodes. Missing optional controls therefore do not stop the application.

## Linux profile VFS

SLiM-CC v2 never deploys the normal workflow directly into the real game `Data` directory.

```text
profile/vfs/layer       winning files from enabled mods
game installation      unchanged base game
profile/vfs/overwrite   files created by the game and tools
          ↓ fuse-overlayfs
profile/vfs/game        virtual game root used for launching
```

The layer is rebuilt from the deterministic deployment plan. The game and external tools must use the virtual game root so they observe the same profile. `fuse-overlayfs` and `fusermount3` are required.

Data mods are placed below `layer/Data`. Recognized game-root installers such as the
Engine Fixes preloader, plus SKSE DLL/EXE components, are placed directly below the virtual
`layer` game root. Both scopes are mounted together and neither writes to the real game
installation.

## Tests

```bash
./scripts/test-frontend.sh
./scripts/test-integration.sh
./scripts/test-e2e.sh
./scripts/test-all.sh
./scripts/test-live-fixtures.sh  # requires explicitly supplied live fixture variables
```

The integration script includes a real rootless FUSE mount and verifies that writes enter the profile overwrite rather than the base game directory.
Live tests use the production download, extraction, import, and VFS planning code. Temporary
Nexus credentials are read from the local database and are never printed or committed.
