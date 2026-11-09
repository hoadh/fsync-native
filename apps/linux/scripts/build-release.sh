#!/usr/bin/env bash
# One command: release artifacts for apps/linux in apps/linux/dist/ (.deb, .rpm, AppImage,
# SHA256SUMS). Everything is built in the ubuntu:22.04 container (Qt 6.2.4, glibc 2.35) and
# nothing is installed on the host. Needs Docker (user in the docker group) and rustup (for the
# host-side guard checks). The PKGBUILD is built from source on Arch, see packaging/arch/.
#
# The commit sha in `--build-info` gets a "-dirty" suffix when the tree has uncommitted changes.
# For a real release set FSYNC_RELEASE_STRICT=1: a dirty tree is then refused.
# Usage: [FSYNC_RELEASE_STRICT=1] apps/linux/scripts/build-release.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
# shellcheck source=lib/common.sh
source "$here/lib/common.sh"

"$here/release-guard.sh"      # env, shipped-feature allowlist, strict clean tree
"$here/check-lock-drift.sh"   # the app links what the root workspace tests

toolchain=$(fsync_toolchain "$repo")
build_tag="fsync-linux-build:ubuntu-2204-$toolchain"
release_tag="fsync-linux-release:ubuntu-2204-$toolchain"
docker build -q -t "$build_tag" --build-arg BASE=ubuntu:22.04 --build-arg RUST_TOOLCHAIN="$toolchain" \
  -f "$here/container/build.Dockerfile" "$here/container" >/dev/null
docker build -q -t "$release_tag" --build-arg BUILD_IMAGE="$build_tag" \
  -f "$here/container/release.Dockerfile" "$here/container" >/dev/null

fsync_ensure_qmlls_ini "$repo"

dist="$repo/apps/linux/dist"
target="$repo/apps/linux/target-ubuntu-2204"
cache="${XDG_CACHE_HOME:-$HOME/.cache}/fsync-linux/ubuntu-2204"
# A failed run must not leave a half-filled dist/ that looks like a release.
trap 'rc=$?; [[ $rc -eq 0 ]] || { rm -rf "$dist"; echo "build failed: dist/ removed" >&2; }' EXIT
rm -rf "$dist"; mkdir -p "$dist" "$target" "$cache/registry" "$cache/git"

# Stable timestamps inside the packages: the time of the commit the tree is based on.
sde=$(git -C "$repo" log -1 --format=%ct 2>/dev/null || date +%s)
sha=$(fsync_git_sha "$repo")
echo "building $sha"

docker run --rm --user "$(id -u):$(id -g)" \
  -v "$repo:/src:ro" \
  -v "$target:/src/apps/linux/target-ubuntu-2204" \
  -v "$dist:/src/apps/linux/dist" \
  -v "$cache:/cargo-home" \
  -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/src/apps/linux/target-ubuntu-2204 \
  -e HOME=/tmp -e SOURCE_DATE_EPOCH="$sde" -e FSYNC_GIT_SHA="$sha" \
  -w /src/apps/linux "$release_tag" /src/apps/linux/scripts/package-in-container.sh

# Intermediate files are not release artifacts.
rm -rf "$dist/stage"
(cd "$dist" && sha256sum -- *.deb *.rpm *.AppImage >SHA256SUMS && cat SHA256SUMS)
echo "release artifacts in $dist"
