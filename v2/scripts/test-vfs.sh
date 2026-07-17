#!/usr/bin/env bash
set -euo pipefail

command -v fuse-overlayfs >/dev/null
command -v fusermount3 >/dev/null

root="$(mktemp -d -t slimcc-vfs-test.XXXXXX)"
cleanup() {
  if mountpoint -q "$root/mount"; then fusermount3 -u "$root/mount"; fi
  rm -rf -- "$root"
}
trap cleanup EXIT

mkdir -p "$root/game/Data" "$root/layer/Data" "$root/upper" "$root/work" "$root/mount"
printf 'base' > "$root/game/Data/base.esm"
printf 'mod' > "$root/layer/Data/mod.esp"
fuse-overlayfs -o "lowerdir=$root/layer:$root/game,upperdir=$root/upper,workdir=$root/work" "$root/mount"

test "$(cat "$root/mount/Data/base.esm")" = "base"
test "$(cat "$root/mount/Data/mod.esp")" = "mod"
printf 'profile' > "$root/mount/Data/generated.ini"
test "$(cat "$root/upper/Data/generated.ini")" = "profile"
test ! -e "$root/game/Data/generated.ini"

echo "VFS integration test passed"
