#!/usr/bin/env bash
# Run a prebuilt fsync-linux binary in a runtime-only distro container (distro Qt) and
# report whether --version and both --smoke modes pass.
# Usage: run-in-container.sh <fedora|archlinux> <path to fsync-linux binary>
set -euo pipefail
image="${1:?usage: $0 <fedora|archlinux> <binary>}"
bin="${2:?missing binary path}"
# Check first: docker -v on a missing path would create a root-owned directory there.
[[ -f "$bin" ]] || { echo "no such binary: $bin" >&2; exit 2; }
bin="$(realpath "$bin")"
here="$(cd "$(dirname "$0")" && pwd)"
dockerfile="$here/container/$image.Dockerfile"
[[ -f "$dockerfile" ]] || { echo "no Dockerfile for $image" >&2; exit 2; }
tag="fsync-linux-run:$image"
docker build -q -t "$tag" -f "$dockerfile" "$here/container" >/dev/null
docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp \
  --mount "type=bind,src=$bin,dst=/opt/fsync/fsync-linux,readonly" "$tag" bash -euc '
    b=/opt/fsync/fsync-linux
    qt=$(ls /usr/lib*/libQt6Core.so.6.* /usr/lib/*/libQt6Core.so.6.* 2>/dev/null | head -1)
    echo "runtime Qt: ${qt##*.so.}"
    "$b" --version
    QT_QPA_PLATFORM=offscreen "$b" --smoke
    FSYNC_SMOKE_CORE=1 QT_QPA_PLATFORM=offscreen "$b" --smoke
    echo "run ok"'
