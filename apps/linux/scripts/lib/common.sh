# shellcheck shell=bash
# Shared helpers for apps/linux/scripts (source it, do not run it).

# fsync_toolchain <repo>: Rust channel pinned in rust-toolchain.toml.
fsync_toolchain() {
  local t
  t=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$1/rust-toolchain.toml")
  [[ -n "$t" ]] || { echo "no channel in $1/rust-toolchain.toml" >&2; return 2; }
  echo "$t"
}

# fsync_ensure_qmlls_ini <repo>: cxx-qt-build writes app/.qmlls.ini into the source tree and the
# repo is mounted read-only in containers, so create it on the host first (gitignored).
fsync_ensure_qmlls_ini() {
  [[ -e "$1/apps/linux/app/.qmlls.ini" ]] || : >"$1/apps/linux/app/.qmlls.ini"
}

# fsync_git_sha <repo>: short HEAD sha, "-dirty" when the tree has uncommitted changes.
fsync_git_sha() {
  local sha
  sha=$(git -C "$1" rev-parse --short HEAD 2>/dev/null) || { echo unknown; return; }
  if [[ -n "$(git -C "$1" status --porcelain 2>/dev/null)" ]]; then echo "$sha-dirty"; else echo "$sha"; fi
}

# fsync_assets <assets.list>: the "<mode> <source> <dest>" lines (comments and blank lines dropped).
fsync_assets() {
  grep -Ev '^[[:space:]]*(#|$)' "$1"
}
