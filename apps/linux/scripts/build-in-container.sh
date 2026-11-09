#!/usr/bin/env bash
# Build apps/linux (release) inside a distro container and run its --smoke checks.
# Usage: build-in-container.sh <ubuntu-2204|debian-12>
# Output: apps/linux/target-<image>/release/fsync-linux (owned by the calling user).
set -euo pipefail
image="${1:?usage: $0 <ubuntu-2204|debian-12>}"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
case "$image" in
  ubuntu-2204) base=ubuntu:22.04 ;;
  debian-12) base=debian:12 ;;
  *) echo "unknown image $image (ubuntu-2204|debian-12)" >&2; exit 2 ;;
esac
# shellcheck source=lib/common.sh
source "$here/lib/common.sh"
toolchain=$(fsync_toolchain "$repo")
tag="fsync-linux-build:$image-$toolchain"
docker build -q -t "$tag" --build-arg BASE="$base" --build-arg RUST_TOOLCHAIN="$toolchain" \
  -f "$here/container/build.Dockerfile" "$here/container" >/dev/null

fsync_ensure_qmlls_ini "$repo"

target="$repo/apps/linux/target-$image"
cache="${XDG_CACHE_HOME:-$HOME/.cache}/fsync-linux/$image"
mkdir -p "$target" "$cache/registry" "$cache/git"

# Repo read-only; target and cargo caches are host-owned bind mounts (no root-owned files).
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$repo:/src:ro" \
  -v "$target:/src/apps/linux/target-$image" \
  -v "$cache:/cargo-home" \
  -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR="/src/apps/linux/target-$image" \
  -e HOME=/tmp -w /src/apps/linux "$tag" bash -euc '
    cargo build --release --locked
    bin="$CARGO_TARGET_DIR/release/fsync-linux"
    "$bin" --version
    QT_QPA_PLATFORM=offscreen "$bin" --smoke
    FSYNC_SMOKE_CORE=1 QT_QPA_PLATFORM=offscreen "$bin" --smoke
    echo "smoke ok"'
