#!/usr/bin/env bash
# Derive the runtime packages of the release binary instead of guessing them (phase 05):
#   - ldd on the binary -> owning package of every resolved shared library
#   - qmlimportscanner over app/qml -> owning package of every QML module it needs
#   - the Qt platform plugins found in the image -> owning package (loaded by name at run time,
#     invisible to ldd)
# Runs in a throw-away container of the target distro; the repo and binary are mounted read-only.
# Usage: derive-runtime-deps.sh <ubuntu-2204|debian-12|fedora> [binary]
# Output: a package list per source on stdout. Paste the result into app/Cargo.toml
# ([package.metadata.deb] depends / [package.metadata.generate-rpm] requires).
set -euo pipefail
distro="${1:?usage: $0 <ubuntu-2204|debian-12|fedora> [binary]}"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
bin="${2:-$repo/apps/linux/target-ubuntu-2204/release/fsync-linux}"
[[ -f "$bin" ]] || { echo "no such binary: $bin" >&2; exit 2; }
bin="$(realpath "$bin")"

case "$distro" in
  ubuntu-2204|debian-12)
    base=ubuntu:22.04; [[ "$distro" == debian-12 ]] && base=debian:12
    # shellcheck source=lib/common.sh
    source "$here/lib/common.sh"
    toolchain=$(fsync_toolchain "$repo")
    image="fsync-linux-build:$distro-$toolchain"
    docker build -q -t "$image" --build-arg BASE="$base" --build-arg RUST_TOOLCHAIN="$toolchain" \
      -f "$here/container/build.Dockerfile" "$here/container" >/dev/null
    ;;
  fedora)
    image=fedora:latest ;;
  *) echo "unknown distro $distro" >&2; exit 2 ;;
esac

docker run --rm -e DISTRO="$distro" \
  --mount "type=bind,src=$bin,dst=/opt/fsync/fsync-linux,readonly" \
  --mount "type=bind,src=$repo/apps/linux/app/qml,dst=/qml,readonly" \
  "$image" bash -euc '
    if [[ $DISTRO == fedora ]]; then
      dnf install -y -q --setopt=install_weak_deps=False qt6-qtbase qt6-qtbase-gui \
        qt6-qtdeclarative qt6-qtdeclarative-devel qt6-qtwayland mesa-libGL fontconfig \
        libxkbcommon binutils >/dev/null
    fi
    # owner_of <path>: package name(s) owning the path or its realpath (usrmerge aliases).
    owner_of() {
      for p in "$1" "$(realpath "$1")"; do
        if [[ $DISTRO == fedora ]]; then r=$(rpm -qf --qf "%{NAME}\n" "$p" 2>/dev/null) || continue
        else r=$(dpkg-query -S "$p" 2>/dev/null | sed "s/:.*//;s/:amd64//" | sort -u) || continue; fi
        [[ -n "$r" ]] && { echo "$r"; return; }
      done
      echo "UNOWNED:$1"
    }
    echo "== distro $DISTRO"
    echo "== direct NEEDED libraries (readelf; apt/dnf pull the transitive ones) -> package"
    for so in $(readelf -d /opt/fsync/fsync-linux | sed -n "s/.*(NEEDED).*\[\(.*\)\]/\1/p" | sort -u); do
      lib=$(ldd /opt/fsync/fsync-linux | awk -v n="$so" "\$1==n {print \$3}")
      [[ -n "$lib" ]] || lib=$(ldd /opt/fsync/fsync-linux | awk -v n="$so" "index(\$1, n) {print \$1}")
      printf "%-60s %s\n" "$so" "$(owner_of "$lib" | tr "\n" " ")"
    done
    echo "== all resolved shared libraries (ldd) -> package"
    ldd /opt/fsync/fsync-linux | awk "/=>/ && \$3 ~ /^\// {print \$3}" | sort -u | while read -r lib; do
      printf "%-60s %s\n" "$lib" "$(owner_of "$lib" | tr "\n" " ")"
    done
    qis=$(ls /usr/lib/qt6/libexec/qmlimportscanner /usr/lib64/qt6/libexec/qmlimportscanner 2>/dev/null | head -1)
    [[ -n $qis ]] || { echo "qmlimportscanner not found" >&2; exit 1; }
    qml=$(ls -d /usr/lib/x86_64-linux-gnu/qt6/qml /usr/lib64/qt6/qml 2>/dev/null | head -1)
    echo "== QML modules (qmlimportscanner $qis, importPath $qml) -> package"
    "$qis" -rootPath /qml -importPath "$qml" 2>/tmp/qis.err | grep -o "\"path\": \"[^\"]*\"" | sed "s/.*: \"//;s/\"//" \
      | sort -u | while read -r d; do
        [[ -f "$d/qmldir" ]] || continue
        printf "%-60s %s\n" "${d#$qml/}" "$(owner_of "$d/qmldir" | tr "\n" " ")"
      done
    echo "== modules the scanner could not find in this image (not installed here, see the deps)"
    sort -u /tmp/qis.err | sed "s/^/   /"
    echo "== Qt platform plugins in this image -> package"
    for f in $(ls -d /usr/lib/x86_64-linux-gnu/qt6/plugins/platforms/*.so /usr/lib64/qt6/plugins/platforms/*.so 2>/dev/null); do
      printf "%-60s %s\n" "${f##*/}" "$(owner_of "$f" | tr "\n" " ")"
    done
    echo "== Qt wayland plugin dirs -> package"
    for f in $(ls -d /usr/lib/x86_64-linux-gnu/qt6/plugins/wayland-*/*.so /usr/lib64/qt6/plugins/wayland-*/*.so 2>/dev/null | head -3); do
      printf "%-60s %s\n" "${f##*/}" "$(owner_of "$f" | tr "\n" " ")"
    done
  '
