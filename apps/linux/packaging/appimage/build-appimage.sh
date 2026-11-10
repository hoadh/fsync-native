#!/usr/bin/env bash
# Builds the AppImage with linuxdeploy + linuxdeploy-plugin-qt (pinned in release.Dockerfile).
# Runs inside the release image. Bundles the build container's Qt 6.2 (ADR 014 decision 6).
# Usage: build-appimage.sh <stage dir with fsync-linux> <output dir> <version>
set -euo pipefail
stage="${1:?stage dir}"; out="${2:?output dir}"; version="${3:?version}"
res=/src/apps/linux/resources
# shellcheck source=../../scripts/lib/common.sh
source /src/apps/linux/scripts/lib/common.sh
work=$(mktemp -d)   # AppDir and linuxdeploy scratch live here, never in the source tree
trap 'rm -rf "$work"' EXIT

# No FUSE in containers: AppImages run unpacked. QMAKE selects Qt 6; QML_SOURCES_PATHS lets the
# plugin scan our QML imports. The Wayland platform plugin is not bundled by default.
export APPIMAGE_EXTRACT_AND_RUN=1
export QMAKE=/usr/bin/qmake6
export QML_SOURCES_PATHS=/src/apps/linux/app/qml
# (libqoffscreen.so: headless runs, the same QPA the smoke checks use.)
export EXTRA_PLATFORM_PLUGINS="libqwayland-egl.so;libqwayland-generic.so;libqoffscreen.so"
export LINUXDEPLOY_OUTPUT_VERSION="$version"
export ARCH=x86_64

cd "$work"
linuxdeploy --appdir AppDir \
  --executable "$stage/fsync-linux" \
  --desktop-file "$res/app.fsync.Fsync.desktop" \
  --icon-file "$res/icons/hicolor/256x256/apps/app.fsync.Fsync.png" \
  --icon-file "$res/icons/hicolor/48x48/apps/app.fsync.Fsync.png" \
  --plugin qt

# plugin-qt deploys the Wayland QPA but not the plugin directories it loads at run time (shell
# integration = xdg-shell, client decoration, EGL buffer integration): copy them, then let
# linuxdeploy resolve their library dependencies. Missing ones would break window creation
# on Wayland while --smoke (offscreen) still passes, so fail the build if one is absent.
qt_plugins=$("$QMAKE" -query QT_INSTALL_PLUGINS)
for d in wayland-shell-integration wayland-decoration-client wayland-graphics-integration-client; do
  [[ -d "$qt_plugins/$d" ]] || { echo "missing Qt plugin dir $qt_plugins/$d" >&2; exit 1; }
  cp -r "$qt_plugins/$d" AppDir/usr/plugins/
  linuxdeploy --appdir AppDir --deploy-deps-only "AppDir/usr/plugins/$d"
done
[[ -e AppDir/usr/plugins/wayland-shell-integration/libxdg-shell.so ]] \
  || { echo "xdg-shell integration plugin not bundled" >&2; exit 1; }

# The license report of the bundled crates travels with the AppImage too.
install -Dm644 "$stage/THIRD-PARTY-LICENSES.txt" \
  AppDir/usr/share/doc/fsync-linux/THIRD-PARTY-LICENSES.txt
# So do our own license texts (the /usr/share/licenses lines of the shared asset list).
while read -r mode src dest; do
  if [[ $dest == /usr/share/licenses/* ]]; then install -Dm"$mode" "/src/apps/linux/$src" "AppDir$dest"; fi
done < <(fsync_assets /src/apps/linux/scripts/lib/assets.list)

linuxdeploy --appdir AppDir --output appimage

shopt -s nullglob
made=("$work"/*.AppImage)
[[ ${#made[@]} -eq 1 ]] || { echo "expected one AppImage, got ${#made[@]}" >&2; exit 1; }
install -m 755 "${made[0]}" "$out/fsync-linux-$version-x86_64.AppImage"
echo "AppImage: $(du -h "$out/fsync-linux-$version-x86_64.AppImage" | cut -f1)"
