#!/usr/bin/env bash
# The .deb and .rpm asset blocks in app/Cargo.toml must install exactly the files of
# scripts/lib/assets.list (cargo-deb reads assets from Cargo.toml only, so the list cannot be
# read there directly). Run by package-in-container.sh before packaging; usable alone.
# Built files (dist/stage/...) and the deb-only copyright file are not part of the list.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
source "$here/lib/common.sh"
manifest="$here/../app/Cargo.toml"
list="$here/lib/assets.list"

# block <toml table header>: the lines of `assets = [ ... ]` in that table, unindented.
block() {
  awk -v hdr="$1" '
    /^\[/ { in_tbl = ($0 == hdr); next }
    in_tbl && /^assets = \[/ { in_arr = 1; next }
    in_arr && /^\]/ { in_arr = 0; next }
    in_arr { sub(/^[ \t]+/, ""); print }' "$manifest"
}
# Cargo.toml paths are relative to app/, the list's to apps/linux; deb destinations have no "/".
skip='dist/stage/|packaging/deb/copyright'
rc=0
# compare <fmt> <toml table header> <printf format of one entry: src, dest, mode>
compare() {
  local want have
  want=$(fsync_assets "$list" | while read -r mode src dest; do
    [[ $1 == deb ]] && dest=${dest#/}
    # shellcheck disable=SC2059  # the entry format is the argument
    printf "$3\n" "$src" "$dest" "$mode"; done | sort)
  have=$(block "$2" | { grep -Ev "$skip" || true; } | sort)
  if [[ "$want" != "$have" ]]; then
    echo "check-assets: [$1] assets in app/Cargo.toml differ from scripts/lib/assets.list (< list, > Cargo.toml):" >&2
    diff <(echo "$want") <(echo "$have") | sed 's/^/  /' >&2 || true
    rc=1
  fi
}
compare deb '[package.metadata.deb]' '["../%s", "%s", "%s"],'
compare rpm '[package.metadata.generate-rpm]' '{ source = "../%s", dest = "%s", mode = "%s" },'
# Every listed source must exist (a typo would otherwise only show up in a package build).
while read -r _ src _; do
  [[ -f "$here/../$src" ]] || { echo "check-assets: missing source $src" >&2; rc=1; }
done < <(fsync_assets "$list")
[[ $rc -eq 0 ]] && echo "check-assets: ok ($(fsync_assets "$list" | wc -l) listed files in deb and rpm)"
exit $rc
