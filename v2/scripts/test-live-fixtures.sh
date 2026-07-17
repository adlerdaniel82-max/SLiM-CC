#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

database="${SLIMCC_DATABASE_PATH:-$HOME/.local/share/slim-cc/slimcc.sqlite}"

if [[ -n "${SLIMCC_LIVE_NEXUS_URL:-}" ]]; then
  if [[ -f "$database" ]] && command -v sqlite3 >/dev/null; then
    SLIMCC_LIVE_NEXUS_API_KEY="$(sqlite3 "$database" "SELECT value FROM app_settings WHERE key='nexus_api_key'")"
    export SLIMCC_LIVE_NEXUS_API_KEY
  fi
  cargo test --manifest-path src-tauri/Cargo.toml \
    live_nexus_download_uses_the_production_backend -- --ignored --nocapture
  unset SLIMCC_LIVE_NEXUS_API_KEY
fi

if [[ -n "${SLIMCC_LIVE_FOMOD_PATH:-}" ]]; then
  cargo test --manifest-path src-tauri/Cargo.toml \
    live_fomod_preview_uses_the_production_extractor_and_parser -- --ignored --nocapture
fi

if [[ -n "${SLIMCC_LIVE_MOD_PATH:-}" ]]; then
  cargo test --manifest-path src-tauri/Cargo.toml \
    live_mod_import_uses_the_production_extractor_and_scanner -- --ignored --nocapture
fi

if [[ -z "${SLIMCC_LIVE_NEXUS_URL:-}${SLIMCC_LIVE_FOMOD_PATH:-}${SLIMCC_LIVE_MOD_PATH:-}" ]]; then
  echo "Set SLIMCC_LIVE_NEXUS_URL, SLIMCC_LIVE_FOMOD_PATH or SLIMCC_LIVE_MOD_PATH." >&2
  exit 2
fi
