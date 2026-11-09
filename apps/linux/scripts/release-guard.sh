#!/usr/bin/env bash
# Refuse a release build that could carry a test-only feature (red team #6). Run first by
# build-release.sh; also usable alone. Layers (the others: compile_error! in app/src/main.rs,
# `--build-info` in package-in-container.sh / smoke-checks.sh / PKGBUILD package()):
#   1. FSYNC_ALLOW_TEST_FEATURES must be unset (it opts a release build into e2e-control).
#   2. ALLOWLIST: every crate of this repo in the release graph (fsync-linux, platform, core,
#      proto, ...) may enable only the features in SHIPPED_FEATURES (none today). Any other
#      feature, known or not, fails closed. Third-party crates are not restricted, but the test
#      feature names are refused anywhere.
#   3. FSYNC_RELEASE_STRICT=1 (real releases): the git tree must be clean.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
manifest="$repo/apps/linux/Cargo.toml"
# Features a shipped build may enable on this repo's own crates. Extend deliberately.
SHIPPED_FEATURES=" default "

if [[ -n "${FSYNC_ALLOW_TEST_FEATURES:-}" ]]; then
  echo "release-guard: FSYNC_ALLOW_TEST_FEATURES is set (='$FSYNC_ALLOW_TEST_FEATURES'); a release build must not allow test features" >&2
  exit 1
fi

if [[ "${FSYNC_RELEASE_STRICT:-}" == 1 && -n "$(git -C "$repo" status --porcelain)" ]]; then
  echo "release-guard: FSYNC_RELEASE_STRICT=1 and the git tree is dirty; commit first" >&2
  git -C "$repo" status --short | head -10 >&2
  exit 1
fi

# Capture first so a cargo failure aborts instead of passing an empty tree.
if ! tree=$(cargo tree --locked --manifest-path "$manifest" -p fsync-linux -e normal,build \
    --prefix none --format '{p} {f}' 2>&1); then
  echo "release-guard: cargo tree failed:" >&2; echo "$tree" >&2; exit 2
fi

bad=""
# Repo crates are the lines with a local path: "name vX.Y.Z (/abs/path) feat1,feat2 [(*)]".
while IFS= read -r line; do
  feats=$(sed -E 's/^[^(]*\([^)]*\) *//; s/ *\(\*\) *$//' <<<"$line")
  IFS=',' read -r -a list <<<"$feats"
  for f in "${list[@]}"; do
    f="${f// /}"
    [[ -z "$f" ]] && continue
    [[ "$SHIPPED_FEATURES" == *" $f "* ]] || bad+="  ${line%% (*}: feature '$f'"$'\n'
  done
done < <(grep -E '^[^ ]+ v[^ ]+ \(/' <<<"$tree")
if [[ -n "$bad" ]]; then
  echo "release-guard: feature outside the shipped allowlist ($SHIPPED_FEATURES) on a repo crate:" >&2
  printf '%s' "$bad" >&2
  exit 1
fi
if hits=$(grep -E '(^|[ ,])(test-hooks|test-relay-ca|e2e-control)([ ,]|$)' <<<"$tree"); then
  echo "release-guard: test-only feature enabled in the release dependency graph:" >&2
  echo "$hits" | sed 's/^/  /' >&2
  exit 1
fi
echo "release-guard: ok (env unset; repo crates enable only$SHIPPED_FEATURES; no test features)"
