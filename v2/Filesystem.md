# Filesystem Strategy for SLiM-CC

Goal: decide how SLiM-CC should move from staging-only deployment to real filesystem deployment, while keeping the path open for a Windows release.

Current state:
- Deployment is still intentionally blocked for the live Skyrim `Data/` folder.
- The codebase already has a staging manifest and a deploy plan with `Copy`, `Symlink`, and `Hardlink` actions.
- Real deployment needs backup, rollback, and final tool validation before it is enabled.

Relevant code paths:
- `src-tauri/src/deploy.rs`
- `src-tauri/src/staging.rs`
- `src-tauri/src/paths.rs`
- `TODO_FOR_CODEX.md`

---

## Option 1: Copy-based deployment

Build the final file tree by copying mod files into a staging tree and then into the live target.

### Pros
- Works on Linux and Windows without special filesystem features.
- Easy to reason about and debug.
- Lowest risk for a first live deployment.
- Backup and rollback can be implemented with simple manifests and copied snapshots.

### Cons
- Uses the most disk space.
- Slower than link-based approaches for large mod lists.
- Every deploy re-writes many files, which is wasteful for iterative testing.

### Estimated effort
- Medium for a safe staging-only continuation.
- Medium-high once backup, rollback, and real `Data/` deployment are added.
- Rough order of magnitude: 3-7 implementation days for a solid first version.

### Platform availability
- Linux: yes
- Windows: yes
- macOS: technically yes, though not a target for this project

### Fit for SLiM-CC
- Best as the compatibility baseline.
- Good if Windows support is a near-term requirement and the priority is reliability over speed.

---

## Option 2: Link-based deployment with platform adapters

Use hardlinks where possible, symlinks where allowed, and Windows junctions or symlink equivalents behind an OS-specific adapter layer.

### Pros
- Much lower disk usage than copy-based deployment.
- Faster deploys and faster iteration.
- Matches the current `DeployAction` model well because the backend already distinguishes copy, symlink, and hardlink behavior.
- Can be made cross-platform if Windows-specific link rules are handled explicitly.

### Cons
- Windows link behavior is more fragile than Linux.
- Symlinks can require elevated permissions or developer mode on Windows.
- Hardlinks have filesystem constraints and cannot cross volumes.
- More edge cases to test and support than a pure copy strategy.

### Estimated effort
- Medium-high.
- Rough order of magnitude: 1-2 weeks to harden properly for Linux and Windows.

### Platform availability
- Linux: yes
- Windows: yes, but only with dedicated adapter logic and feature checks
- macOS: possible, but not a project target

### Fit for SLiM-CC
- Best compromise if SLiM-CC should remain fast on Linux and still have a real Windows build.
- This is the strongest candidate if the plan is to support both platforms with one codebase.

---

## Option 3: Filesystem virtualization / overlay layer

Use a virtual or overlay filesystem model for the mod set, with the live game folder seeing a composed view rather than a fully materialized copy.

### Pros
- Cleanest mental model for enable/disable behavior.
- Very fast switching between profiles.
- Good fit for a power-user Linux workflow if the environment is controlled.

### Cons
- Linux-only or Linux-first in practice.
- Windows support would need a separate implementation path, which is expensive and brittle.
- Harder to package, support, and debug.
- More moving parts before the project is ready for real users.

### Estimated effort
- High.
- Rough order of magnitude: several weeks, plus platform-specific testing and support work.

### Platform availability
- Linux: yes, with kernel/FUSE/overlay-dependent implementation choices
- Windows: no practical shared implementation
- macOS: not a realistic target

### Fit for SLiM-CC
- Good only if SLiM-CC stays Linux-only.
- Not the right choice if Windows support is a serious near-term goal.

---

## Recommendation

If Windows support is genuinely on the table, the safest path is:

1. Keep the current staging model as the internal source of truth.
2. Make Option 2 the primary real-deployment strategy.
3. Keep Option 1 as the fallback when links are not possible.
4. Treat Option 3 as a later Linux-only optimization, not the cross-platform base.

That gives SLiM-CC:
- a portable baseline for Windows,
- a faster Linux path,
- and a realistic way to reach real filesystem deployment without rewriting the backend again later.

## Practical gate before live deployment

Before enabling real `Data/` writes, the backend should still have:
- confirmed LOOT, SSEEdit/xEdit, and Nemesis launch behavior,
- a backup and rollback plan,
- deterministic staging manifests,
- and a final live deploy test on the target platform.

