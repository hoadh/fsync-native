#!/usr/bin/env bash
# Builds FsyncMac.app (release, ad-hoc signed with the hardened runtime).
#
#   build-fsync-mac-app.sh [--api URL] [--relay URL] [--bundle-id ID] [--out DIR] [--version X.Y.Z]
#
# Defaults: api http://127.0.0.1:8080, no relay, app.fsync.mac, apps/macos/build/, 0.1.0.
# Needs apps/macos/Packages/FsyncCoreKit/FsyncFFI.xcframework (run scripts/build-xcframework.sh first).
set -euo pipefail

MAC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
API="http://127.0.0.1:8080"
RELAY=""
BUNDLE_ID="app.fsync.mac"
OUT="$MAC/build"
VERSION="0.1.0"

usage() { sed -n '2,6p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --api) API="${2:?--api needs a value}"; shift 2 ;;
    --relay) RELAY="${2:?--relay needs a value}"; shift 2 ;;
    --bundle-id) BUNDLE_ID="${2:?--bundle-id needs a value}"; shift 2 ;;
    --out) OUT="${2:?--out needs a value}"; shift 2 ;;
    --version) VERSION="${2:?--version needs a value}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done
[[ "$BUNDLE_ID" =~ ^[A-Za-z0-9.-]+$ ]] || { echo "invalid bundle id: $BUNDLE_ID" >&2; exit 2; }

if [[ ! -d "$MAC/Packages/FsyncCoreKit/FsyncFFI.xcframework" ]]; then
  echo "FsyncFFI.xcframework is missing. Run scripts/build-xcframework.sh first." >&2
  exit 1
fi

echo "building FsyncMac (release)"
swift build -c release --product FsyncMac --package-path "$MAC/FsyncMac"
BIN="$(swift build -c release --product FsyncMac --package-path "$MAC/FsyncMac" --show-bin-path)/FsyncMac"
[[ -x "$BIN" ]] || { echo "missing binary $BIN" >&2; exit 1; }

mkdir -p "$OUT" && OUT="$(cd "$OUT" && pwd)"
APP="$OUT/FsyncMac.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/FsyncMac"

PLIST="$APP/Contents/Info.plist"
plutil -create xml1 "$PLIST"
plist_str() { plutil -insert "$1" -string "$2" "$PLIST"; }
plist_str CFBundleIdentifier "$BUNDLE_ID"
plist_str CFBundleName "fsync"
plist_str CFBundleDisplayName "fsync"
plist_str CFBundleExecutable "FsyncMac"
plist_str CFBundlePackageType "APPL"
plist_str CFBundleShortVersionString "$VERSION"
plist_str CFBundleVersion "$VERSION"
plist_str LSMinimumSystemVersion "13.0"
plutil -insert LSUIElement -bool true "$PLIST"
plist_str FsyncApiBase "$API"
[[ -z "$RELAY" ]] || plist_str FsyncRelayUrl "$RELAY"
plutil -lint "$PLIST" >/dev/null

# Ad-hoc identity changes on every build; Keychain ACLs and login-item approval may need renewing.
codesign --force --options runtime --timestamp=none -s - "$APP"
codesign --verify --strict "$APP"

echo "$APP"
