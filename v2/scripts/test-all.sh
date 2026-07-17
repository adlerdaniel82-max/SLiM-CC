#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
npm run build
./scripts/test-frontend.sh
./scripts/test-integration.sh
./scripts/test-e2e.sh
