#!/usr/bin/env bash
# Checks run INSIDE a clean distro container, as an unprivileged user, after the package (or
# AppImage) is installed by smoke-install.sh. Prints PASS/FAIL per check, exits 1 on any FAIL.
# Usage: smoke-checks.sh installed | appimage <AppImage file | extracted squashfs-root dir> | bare
#   installed  fsync-linux from PATH plus the installed package files
#   appimage   the AppImage run unpacked (no FUSE); a directory runs its AppRun (negative tests)
#   bare       only the binary checks (negative tests: a loose binary without package files)
set -uo pipefail
mode="${1:?mode}"
appimage="${2:-}"
fail=0
pass() { echo "PASS  $*"; }
bad()  { echo "FAIL  $*"; fail=1; }

export HOME; HOME=$(mktemp -d)
# appimage_cmd: how to start the AppImage (a file runs extract-and-run, a directory its AppRun).
appimage_cmd() {
  if [[ -d $appimage ]]; then "$appimage/AppRun" "$@"
  else APPIMAGE_EXTRACT_AND_RUN=1 "$appimage" "$@"; fi
}
run() {
  if [[ $mode == appimage ]]; then appimage_cmd "$@"; else fsync-linux "$@"; fi
}

# 1. --version prints exactly "fsync-linux <semver>" and exits 0
if v=$(run --version 2>&1) && [[ "$v" =~ ^fsync-linux\ [0-9]+\.[0-9]+\.[0-9]+ ]]; then pass "--version: $v"
else bad "--version ($v)"; fi

# 2. --build-info lists NO test features (red team #6; features must be exactly empty)
if info=$(run --build-info 2>&1); then
  echo "      build-info: $info"
  if [[ "$info" == *'"features":[]'* ]]; then pass "--build-info has no features"
  else bad "--build-info lists features: $info"; fi
else bad "--build-info failed: $info"; fi

# 3. --smoke (QML loads without warnings) and the core variant (SQLCipher/OpenSSL open+close)
if out=$(QT_QPA_PLATFORM=offscreen run --smoke 2>&1); then pass "--smoke"
else bad "--smoke exit $?: $out"; fi
if out=$(FSYNC_SMOKE_CORE=1 QT_QPA_PLATFORM=offscreen run --smoke 2>&1); then pass "FSYNC_SMOKE_CORE=1 --smoke"
else bad "FSYNC_SMOKE_CORE=1 --smoke exit $?: $out"; fi

desktop_to_check=""
docfile=""
case "$mode" in
  installed)
    if missing=$(ldd "$(command -v fsync-linux)" | grep 'not found'); then bad "unresolved libraries: $missing"
    else pass "ldd: all libraries resolve"; fi
    desktop=/usr/share/applications/app.fsync.Fsync.desktop
    [[ -f $desktop ]] && pass "desktop file installed" || bad "missing $desktop"
    for f in /usr/share/icons/hicolor/48x48/apps/app.fsync.Fsync.png \
             /usr/share/icons/hicolor/256x256/apps/app.fsync.Fsync.png \
             /usr/share/icons/hicolor/scalable/apps/app.fsync.Fsync.svg; do
      [[ -f $f ]] && pass "installed $f" || bad "missing $f"
    done
    for docfile in /usr/share/doc/fsync-linux/THIRD-PARTY-LICENSES.txt \
                   /usr/share/licenses/fsync-linux/THIRD-PARTY-LICENSES.txt; do
      [[ -s $docfile ]] && break
    done
    perms=$(stat -c '%a' "$(command -v fsync-linux)")
    [[ $perms == 755 ]] && pass "binary mode 755 (no setuid)" || bad "binary mode $perms"
    desktop_to_check=$desktop ;;
  appimage)
    # Qt must come from the AppImage: the host has none (ADR: AppImage works without Qt installed).
    if ldconfig -p | grep -q 'libQt6Core'; then bad "host has libQt6Core: not a Qt-free environment"
    else pass "host has no Qt libraries"; fi
    if [[ -d $appimage ]]; then root=$appimage; ex=""
    else
      ex=$(mktemp -d); root=$ex/squashfs-root
      # Extraction must work or the file checks below mean nothing: it is a FAIL, not a skip.
      # (env -u: the runtime only tests whether the variable is SET.)
      if ( cd "$ex" && env -u APPIMAGE_EXTRACT_AND_RUN "$appimage" --appimage-extract >/dev/null 2>"$ex/err" ); then
        pass "AppImage extracts"
      else bad "AppImage extraction failed: $(head -3 "$ex/err")"; fi
    fi
    desktop_to_check=$(ls "$root"/*.desktop 2>/dev/null | head -1)
    docfile=$root/usr/share/doc/fsync-linux/THIRD-PARTY-LICENSES.txt
    # The Wayland QPA plugin must be bundled AND its code must run. With no compositor a working
    # plugin ends in "Failed to create wl_display"; a missing plugin or an unresolved dependency
    # ends in "Cannot load library ...qwayland" / "Could not find the Qt platform plugin". The
    # verdict uses the FULL debug output (never an excerpt).
    plug=$(QT_QPA_PLATFORM=wayland QT_DEBUG_PLUGINS=1 WAYLAND_DISPLAY=none XDG_RUNTIME_DIR=$HOME \
      timeout 30 bash -c "$(declare -f appimage_cmd); appimage='$appimage'; appimage_cmd" 2>&1)
    grep -iE 'wayland|Could not|Cannot load' <<<"$plug" | head -6 | sed 's/^/      qpa: /' | cut -c1-200
    if grep -qE 'Cannot load library .*qwayland|Could not find the Qt platform plugin' <<<"$plug"; then
      bad "wayland QPA plugin missing or unloadable"
    elif grep -q 'Failed to create wl_display' <<<"$plug"; then
      pass "wayland QPA plugin loaded and ran (no compositor: Failed to create wl_display)"
    else bad "wayland QPA: no evidence the plugin code ran"; fi
    [[ -n "$ex" ]] && trap 'rm -rf "$ex"' EXIT ;;
  bare) ;;
  *) bad "unknown mode $mode" ;;
esac

# 4. license report of the bundled crates shipped with the package
if [[ $mode != bare ]]; then
  [[ -s ${docfile:-/nonexistent} ]] && pass "third-party license report: $docfile" \
    || bad "missing THIRD-PARTY-LICENSES.txt"
fi

# 5. desktop-file-validate clean (a missing desktop file is a FAIL in every packaged mode)
if [[ $mode != bare ]]; then
  if [[ -z $desktop_to_check || ! -f $desktop_to_check ]]; then bad "no .desktop file to validate"
  elif out=$(desktop-file-validate "$desktop_to_check" 2>&1) && [[ -z "$out" ]]; then pass "desktop-file-validate"
  else bad "desktop-file-validate: $out"; fi
fi

[[ $fail -eq 0 ]] && echo "ALL CHECKS PASSED" || echo "CHECKS FAILED"
exit $fail
