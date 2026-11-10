#!/usr/bin/env bash
# Install a release artifact in a CLEAN distro container and run the headless checks of
# container/smoke-checks.sh (version, --build-info without test features, --smoke twice,
# license report, desktop-file-validate, installed files). Exit 0 only if everything passes.
#
# Usage: smoke-install.sh <target> <artifact>
#   ubuntu-2204 | ubuntu-2404 | debian-12   <file.deb>    (apt, --no-install-recommends)
#   fedora                                  <file.rpm>    (dnf, weak deps off)
#   appimage | appimage-fedora | appimage-arch
#                                           <file.AppImage> on ubuntu:22.04 / fedora / archlinux
#                                                         with base desktop libraries and NO Qt
#   arch                                    <PKGBUILD>    makepkg of the CURRENT tree (local mode),
#                                                         install, namcap (errors fail the run)
#   fedora                                  <loose binary> negative tests: runtime image with
#                                                         distro Qt, no package, binary checks only
# The artifact is always mounted at a fixed in-container path (/pkg/artifact): its file name never
# reaches a shell.
set -euo pipefail
target="${1:?usage: $0 <ubuntu-2204|ubuntu-2404|debian-12|fedora|appimage[-fedora|-arch]|arch> <artifact>}"
artifact="${2:?missing artifact}"
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
# Check first: a docker bind mount of a missing path would create a root-owned directory.
[[ -f "$artifact" ]] || { echo "no such artifact: $artifact" >&2; exit 2; }
artifact="$(realpath "$artifact")"
name="$(basename "$artifact")"
checks="$here/container/smoke-checks.sh"
assets="$here/lib/assets.list"
# shellcheck source=lib/common.sh
source "$here/lib/common.sh"

# Unprivileged runner shared by every target: root installs, uid 1000 runs the checks.
as_user='setpriv --reuid=1000 --regid=1000 --clear-groups'

run_container() { # <image> <script> [docker args...]
  local image="$1" script="$2"; shift 2
  docker run --rm -e HOME=/root "$@" \
    --mount "type=bind,src=$checks,dst=/smoke/smoke-checks.sh,readonly" \
    --mount "type=bind,src=$assets,dst=/smoke/assets.list,readonly" \
    --mount "type=bind,src=$artifact,dst=/pkg/artifact,readonly" \
    "$image" bash -euc "$script"
}
need_ext() { [[ "$name" == *"$1" ]] || { echo "$target needs a $1 file" >&2; exit 2; }; }

# AppImage: install base desktop libraries (NO Qt) with <install cmd>, then run the checks.
appimage_smoke() { # <image> <install command>
  need_ext .AppImage
  run_container "$1" "
    $2
    cp /pkg/artifact /opt/fsync.AppImage && chmod 755 /opt/fsync.AppImage
    $as_user /smoke/smoke-checks.sh appimage /opt/fsync.AppImage"
}

case "$target" in
  ubuntu-2204|ubuntu-2404|debian-12)
    need_ext .deb
    case "$target" in ubuntu-2204) image=ubuntu:22.04 ;; ubuntu-2404) image=ubuntu:24.04 ;; *) image=debian:12 ;; esac
    run_container "$image" "
      export DEBIAN_FRONTEND=noninteractive
      # Ubuntu container images exclude /usr/share/doc from dpkg; a real system keeps it.
      rm -f /etc/dpkg/dpkg.cfg.d/excludes
      cp /pkg/artifact /tmp/fsync.deb
      apt-get update -qq >/dev/null
      apt-get install -y -qq --no-install-recommends /tmp/fsync.deb desktop-file-utils >/dev/null
      echo \"installed Depends: \$(dpkg-query -W -f='\${Depends}' fsync-linux)\"
      echo \"Qt: \$(dpkg -l 'libqt6core6*' | awk '/^ii/ {print \$2, \$3}')\"
      $as_user /smoke/smoke-checks.sh installed" ;;

  fedora)
    if [[ "$name" == *.rpm ]]; then
      run_container fedora:latest "
        cp /pkg/artifact /tmp/fsync.rpm
        dnf install -y -q --setopt=install_weak_deps=False --setopt=tsflags= /tmp/fsync.rpm desktop-file-utils util-linux >/dev/null
        echo \"installed Requires: \$(rpm -q --requires fsync-linux | tr '\n' ',')\"
        echo \"Recommends: \$(rpm -q --recommends fsync-linux | tr '\n' ',')\"
        echo \"Qt: \$(rpm -q qt6-qtbase)\"
        $as_user /smoke/smoke-checks.sh installed"
    else # loose binary (negative tests): reuse the runtime image of run-in-container.sh
      docker build -q -t fsync-linux-run:fedora -f "$here/container/fedora.Dockerfile" "$here/container" >/dev/null
      # The runtime image has no setpriv: docker --user drops privileges instead. The binary
      # goes to /usr/local/bin (a second mount, read-only).
      run_container fsync-linux-run:fedora "/smoke/smoke-checks.sh bare" --user 1000:1000 \
        --mount "type=bind,src=$artifact,dst=/usr/local/bin/fsync-linux,readonly"
    fi ;;

  appimage)
    appimage_smoke ubuntu:22.04 "export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >/dev/null
    apt-get install -y -qq --no-install-recommends desktop-file-utils libgl1 libopengl0 libegl1 \
      libfontconfig1 libxkbcommon0 libdbus-1-3 libglib2.0-0 libharfbuzz0b libfreetype6 libice6 libsm6 \
      fonts-dejavu-core >/dev/null" ;;
  appimage-fedora)
    appimage_smoke fedora:latest "dnf install -y -q --setopt=install_weak_deps=False desktop-file-utils \
      util-linux mesa-libGL libglvnd-opengl libglvnd-egl fontconfig libxkbcommon harfbuzz freetype \
      libICE libSM dbus-libs glib2 dejavu-sans-fonts >/dev/null" ;;
  appimage-arch)
    appimage_smoke archlinux:latest "pacman -Syu --noconfirm --needed desktop-file-utils libglvnd fontconfig \
      libxkbcommon harfbuzz freetype2 libice libsm dbus glib2 ttf-dejavu >/dev/null" ;;

  arch)
    [[ "$name" == PKGBUILD ]] || { echo "arch needs the PKGBUILD file" >&2; exit 2; }
    work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
    # Tarball of the working tree (tracked + untracked, minus ignored); top directory `fsync/`.
    # --ignore-failed-read: tracked files deleted in the working tree are skipped.
    (cd "$repo" && git ls-files -z --cached --others --exclude-standard \
      | tar --null -T - --ignore-failed-read --transform 's,^,fsync/,' -czf "$work/fsync-src.tar.gz" 2>/dev/null)
    # The tarball has no .git: the host sha goes in FSYNC_GIT_SHA (--build-info must show it).
    sha=$(fsync_git_sha "$repo")
    [[ $sha =~ ^[0-9a-f]{7,40}(-dirty)?$ ]] || { echo "unexpected git sha '$sha'" >&2; exit 2; }
    # BUILDDIR with a space: the rustc remap flags must survive it (review L3).
    run_container archlinux:latest "
      pacman -Syu --noconfirm --needed base-devel git cargo lld openssl qt6-base qt6-declarative qt6-tools \
        qt6-wayland perl cargo-about namcap desktop-file-utils ttf-dejavu hicolor-icon-theme >/dev/null
      useradd -m builder && install -d -o builder -g builder /build
      cp /pkg/artifact /build/PKGBUILD && cp /in/fsync-src.tar.gz /build/ && chown builder: /build/*
      cd /build
      if su builder -c 'FSYNC_LOCAL_SRC=/build/fsync-src.tar.gz makepkg --printsrcinfo' >/dev/null 2>&1; then
        echo 'FAIL  --printsrcinfo accepted FSYNC_LOCAL_SRC (a file:// source could be published)'; exit 1
      fi
      echo 'PASS  --printsrcinfo refused with FSYNC_LOCAL_SRC set'
      # Release mode (placeholder commit replaced by a well-formed dummy) for .SRCINFO and namcap.
      install -d -o builder /tmp/nc
      sed 's/^_commit=.*/_commit=0123456789abcdef0123456789abcdef01234567/' /build/PKGBUILD >/tmp/nc/PKGBUILD
      (cd /tmp/nc && su builder -c 'makepkg --printsrcinfo' >/tmp/nc/.SRCINFO)
      if grep -q 'file://' /tmp/nc/.SRCINFO; then echo 'FAIL  .SRCINFO has a file:// source'; exit 1; fi
      echo \"PASS  release .SRCINFO: \$(grep -c . /tmp/nc/.SRCINFO) lines, \$(grep -m1 'source = ' /tmp/nc/.SRCINFO)\"
      su builder -c \"BUILDDIR='/build/b d' FSYNC_GIT_SHA=$sha FSYNC_LOCAL_SRC=/build/fsync-src.tar.gz makepkg -f --noconfirm\"
      pkg=\$(ls /build/fsync-linux-*.pkg.tar.zst)
      echo \"package: \$pkg (\$(du -h \$pkg | cut -f1))\"
      pacman -U --noconfirm \"\$pkg\" >/dev/null
      echo \"Qt: \$(pacman -Q qt6-base)  OpenSSL: \$(pacman -Q openssl)\"
      # Remapped: no Rust source path under the build dir, nothing from the builder's home.
      if strings /usr/bin/fsync-linux | grep -E '/build/b d/.*\\.rs|/home/builder' | head -3 | grep .; then
        echo 'FAIL  build paths left in the binary'; exit 1
      fi
      echo 'PASS  no build dir or builder home paths in the binary'
      echo '--- namcap PKGBUILD'
      namcap /tmp/nc/PKGBUILD | tee /tmp/namcap.out || true
      echo '--- namcap package'; namcap \"\$pkg\" | tee -a /tmp/namcap.out || true
      # Warnings are accepted (ADR 014); an error (E:) or an unparsable PKGBUILD fails the run.
      if grep -E ' E: |^Error' /tmp/namcap.out; then echo 'FAIL  namcap reported errors'; exit 1; fi
      echo 'PASS  namcap: no errors'
      $as_user /smoke/smoke-checks.sh installed" \
      -e FSYNC_EXPECT_GIT_SHA="$sha" --mount "type=bind,src=$work,dst=/in,readonly" ;;

  *) echo "unknown target $target" >&2; exit 2 ;;
esac
