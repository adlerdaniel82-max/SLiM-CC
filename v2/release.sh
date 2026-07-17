#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: ./release.sh current|fix|feature [commit message]"
}

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(git -C "$APP_DIR" rev-parse --show-toplevel 2>/dev/null || printf '%s' "$APP_DIR")"
RPM_GPG_FINGERPRINT="${SLIMCC_RPM_GPG_FINGERPRINT:-E9484A7ED097349A9BCB11723259A8DDD8BA199D}"
KIND="${1:-}"
[[ "$KIND" =~ ^(current|fix|feature)$ ]] || { usage >&2; exit 2; }
shift

current_version="$(node -p "require('$APP_DIR/package.json').version")"
IFS=. read -r major minor patch <<< "$current_version"
[[ "$major" -ge 2 ]] || { echo "[release] v2 release script refuses v1 version $current_version" >&2; exit 1; }
case "$KIND" in
  current) next_version="$current_version" ;;
  fix) next_version="$major.$minor.$((patch + 1))" ;;
  feature) next_version="$major.$((minor + 1)).0" ;;
esac
commit_message="${*:-Release SLiM-CC v2 $next_version}"

cd "$APP_DIR"
if [[ "$next_version" != "$current_version" ]]; then
  node - "$next_version" <<'NODE'
const fs = require("fs");
const version = process.argv[2];
for (const path of ["package.json", "package-lock.json", "src-tauri/tauri.conf.json"]) {
  const value = JSON.parse(fs.readFileSync(path, "utf8"));
  value.version = version;
  if (value.packages?.[""]) value.packages[""].version = version;
  fs.writeFileSync(path, JSON.stringify(value, null, 2) + "\n");
}
NODE
  sed -i -E "0,/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/s//version = \"$next_version\"/" src-tauri/Cargo.toml
  cargo update --manifest-path src-tauri/Cargo.toml -p slim-cc-v2
fi

command -v fuse-overlayfs >/dev/null || { echo "[release] fuse-overlayfs fehlt" >&2; exit 1; }
command -v fusermount3 >/dev/null || { echo "[release] fusermount3 fehlt" >&2; exit 1; }
gpg --list-secret-keys --with-colons "$RPM_GPG_FINGERPRINT" | grep -q '^sec:' || { echo "[release] RPM signing key fehlt" >&2; exit 1; }
export TAURI_SIGNING_RPM_KEY="$(gpg --export-secret-keys --armor "$RPM_GPG_FINGERPRINT")"
unset TAURI_SIGNING_RPM_KEY_PASSPHRASE

echo "[release] Build und Tests für $next_version"
./scripts/test-all.sh
echo "[release] RPM und AppImage"
ARCH=x86_64 NO_STRIP=1 npm run tauri -- build --bundles rpm,appimage

artifact_dir="$APP_DIR/release-artifacts/v$next_version"
mkdir -p "$artifact_dir"
find src-tauri/target/release/bundle/rpm -maxdepth 1 -type f -name '*.rpm' -exec cp -f {} "$artifact_dir/" \;
find src-tauri/target/release/bundle/appimage -maxdepth 1 -type f -name '*.AppImage' -exec cp -f {} "$artifact_dir/" \;
test -n "$(find "$artifact_dir" -maxdepth 1 -type f -name '*.rpm' -print -quit)"
test -n "$(find "$artifact_dir" -maxdepth 1 -type f -name '*.AppImage' -print -quit)"
(cd "$artifact_dir" && sha256sum ./*.rpm ./*.AppImage > SHA256SUMS)

if [[ -n "${SLIMCC_MIRROR_DIR:-}" ]]; then
  mkdir -p "$SLIMCC_MIRROR_DIR"
  cp -f "$artifact_dir"/* "$SLIMCC_MIRROR_DIR/"
  gpg --export --armor "$RPM_GPG_FINGERPRINT" > "$SLIMCC_MIRROR_DIR/RPM-GPG-KEY-SLiM-CC-v2"
fi

cd "$PROJECT_ROOT"
git add .gitignore v2
if ! git diff --cached --quiet; then git commit -m "$commit_message"; fi
if git remote get-url origin >/dev/null 2>&1; then git push; fi
echo "[release] SLiM-CC v2 $next_version: $artifact_dir"
