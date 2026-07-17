#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
cargo test --manifest-path src-tauri/Cargo.toml
./scripts/test-vfs.sh
