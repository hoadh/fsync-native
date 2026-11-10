#!/usr/bin/env bash
# Fail when fsync-core resolves any crate to another version in apps/linux than in the root
# workspace: the Linux app must link what the root `cargo test --workspace` exercises.
# Fix drift by re-seeding: cp Cargo.lock apps/linux/Cargo.lock && (cd apps/linux && cargo build).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
# Normal + build deps (build deps include the vendored OpenSSL/SQLCipher sources). jobserver is
# excluded: it only appears because cxx-qt enables cc's "parallel" feature (build-time only).
core_tree() { # "name vX.Y.Z" per line
  local out
  if ! out=$(cargo tree --locked --manifest-path "$1" -p fsync-core -e normal,build \
      --prefix none --format '{p}' 2>&1); then
    echo "cargo tree failed for $1:" >&2; echo "$out" >&2; return 2
  fi
  sed 's/ (.*$//' <<<"$out" | grep -v '^jobserver ' | sort -u
}
root=$(core_tree "$repo/Cargo.toml")
app=$(core_tree "$repo/apps/linux/Cargo.toml")
# Fail on a crate of the root tree that is missing or at another version in apps/linux.
# Extra crates (feature unification: e.g. oo7 enables inout/block-padding) are listed only.
missing=$(comm -23 <(echo "$root") <(echo "$app"))
extra=$(comm -13 <(echo "$root") <(echo "$app"))
[[ -n "$extra" ]] && echo "note: extra crates in apps/linux (feature unification):" && sed 's/^/  /' <<<"$extra"
if [[ -z "$missing" ]]; then
  echo "lock drift: none ($(wc -l <<<"$root") crates of fsync-core's tree match)"
else
  echo "lock drift: fsync-core's tree differs (root version missing in apps/linux):"
  sed 's/^/  /' <<<"$missing"
  exit 1
fi
