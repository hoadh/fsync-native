#!/usr/bin/env bash
# Builds FsyncMac.app (release, hardened runtime; ad-hoc signed unless --sign is given).
#
#   build-fsync-mac-app.sh [--api URL] [--relay URL] [--bundle-id ID] [--out DIR] [--version X.Y.Z]
#   --relay takes one URL or a comma-separated list (e.g. US and Singapore relays).
#                          [--sign IDENTITY --profile FILE [--notary-profile NAME]]
#
# Defaults: api http://127.0.0.1:8080, no relay, app.fsync.mac, apps/macos/build/, 0.1.0, ad-hoc.
# --sign: "Developer ID Application: ..." identity; needs --profile (Developer ID provisioning profile
#   for the bundle id), which grants the data-protection Keychain entitlements.
# --notary-profile: notarytool keychain profile (xcrun notarytool store-credentials); notarizes + staples.
# Needs apps/macos/Packages/FsyncCoreKit/FsyncFFI.xcframework (run scripts/build-xcframework.sh first).
set -euo pipefail

MAC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
API="http://127.0.0.1:8080"
RELAY=""
BUNDLE_ID="app.fsync.mac"
OUT="$MAC/build"
VERSION="0.1.0"
IDENTITY=""
PROFILE=""
NOTARY=""

usage() { sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }
# The value after an option; a missing one or another option (`--sign --profile x`) is a usage error.
val() { [[ -n "${2:-}" && "$2" != -* ]] || { echo "$1 needs a value" >&2; exit 2; }; printf '%s' "$2"; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --api) API="$(val --api "${2:-}")" || exit 2; shift 2 ;;
    --relay) RELAY="$(val --relay "${2:-}")" || exit 2; shift 2 ;;
    --bundle-id) BUNDLE_ID="$(val --bundle-id "${2:-}")" || exit 2; shift 2 ;;
    --out) OUT="$(val --out "${2:-}")" || exit 2; shift 2 ;;
    --version) VERSION="$(val --version "${2:-}")" || exit 2; shift 2 ;;
    --sign) IDENTITY="$(val --sign "${2:-}")" || exit 2; shift 2 ;;
    --profile) PROFILE="$(val --profile "${2:-}")" || exit 2; shift 2 ;;
    --notary-profile) NOTARY="$(val --notary-profile "${2:-}")" || exit 2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done
[[ "$BUNDLE_ID" =~ ^[A-Za-z0-9.-]+$ ]] || { echo "invalid bundle id: $BUNDLE_ID" >&2; exit 2; }
if [[ -n "$IDENTITY" && -z "$PROFILE" ]] || [[ -z "$IDENTITY" && -n "$PROFILE$NOTARY" ]]; then
  echo "--sign and --profile go together (--notary-profile needs both)" >&2; exit 2
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
TEAM=""
if [[ -n "$PROFILE" ]]; then
  [[ -f "$PROFILE" ]] || { echo "missing profile $PROFILE" >&2; exit 1; }
  security cms -D -i "$PROFILE" > "$WORK/profile.plist"
  TEAM="$(plutil -extract TeamIdentifier.0 raw "$WORK/profile.plist")"
  APP_ID="$(plutil -extract 'Entitlements.com\.apple\.application-identifier' raw "$WORK/profile.plist")"
  # A wildcard or other App ID would sign fine but leave the Keychain on the fallback store.
  [[ "$APP_ID" == "$TEAM.$BUNDLE_ID" ]] || { echo "profile is for $APP_ID, not $TEAM.$BUNDLE_ID" >&2; exit 1; }
  EXPIRES="$(plutil -extract ExpirationDate raw "$WORK/profile.plist")"
  [[ "$EXPIRES" > "$(date -u +%Y-%m-%dT%H:%M:%SZ)" ]] || { echo "profile expired on $EXPIRES" >&2; exit 1; }
fi

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
plist_str LSMinimumSystemVersion "14.0"
plutil -insert LSUIElement -bool true "$PLIST"
plist_str FsyncApiBase "$API"
[[ -z "$RELAY" ]] || plist_str FsyncRelayUrl "$RELAY"
plutil -lint "$PLIST" >/dev/null

# A signature that passes `codesign --verify` but is not a Developer ID cert of the profile's team,
# or a cert the profile does not list, gets the app killed at launch (restricted entitlements).
check_signer() {
  local info leaf i cert
  info="$(codesign -dvv "$APP" 2>&1)"
  grep -q "^Authority=Developer ID Application: " <<<"$info" || { echo "not a Developer ID Application identity" >&2; exit 1; }
  grep -qx "TeamIdentifier=$TEAM" <<<"$info" || { echo "signing identity is not from team $TEAM" >&2; exit 1; }
  codesign -d --extract-certificates="$WORK/cert" "$APP" 2>/dev/null
  leaf="$(base64 -i "$WORK/cert0" | tr -d '\n')"
  for ((i = 0; ; i++)); do
    cert="$(plutil -extract "DeveloperCertificates.$i" raw "$WORK/profile.plist" 2>/dev/null)" ||
      { echo "signing certificate is not in the provisioning profile" >&2; exit 1; }
    [[ "$(tr -d '\n' <<<"$cert")" == "$leaf" ]] && return 0
  done
}

if [[ -z "$IDENTITY" ]]; then
  # Ad-hoc identity changes on every build; Keychain ACLs and login-item approval may need renewing.
  codesign --force --options runtime --timestamp=none -s - "$APP"
else
  # application-identifier + keychain-access-groups enable the data-protection Keychain; the
  # embedded profile is what makes these restricted entitlements valid outside the App Store.
  cp "$PROFILE" "$APP/Contents/embedded.provisionprofile"
  chmod 644 "$APP/Contents/embedded.provisionprofile"
  ENT="$WORK/FsyncMac.entitlements"
  plutil -create xml1 "$ENT"
  plutil -insert 'com\.apple\.application-identifier' -string "$TEAM.$BUNDLE_ID" "$ENT"
  plutil -insert 'com\.apple\.developer\.team-identifier' -string "$TEAM" "$ENT"
  plutil -insert keychain-access-groups -array "$ENT"
  plutil -insert keychain-access-groups.0 -string "$TEAM.$BUNDLE_ID" "$ENT"
  codesign --force --options runtime --timestamp --entitlements "$ENT" -s "$IDENTITY" "$APP"
  check_signer
fi
codesign --verify --strict "$APP"

if [[ -n "$NOTARY" ]]; then
  echo "notarizing (can take a few minutes)"
  ditto -c -k --keepParent "$APP" "$WORK/FsyncMac.zip"
  # notarytool can exit 0 on an Invalid result, so the status is checked explicitly.
  RESULT="$(xcrun notarytool submit "$WORK/FsyncMac.zip" --keychain-profile "$NOTARY" --wait --timeout 30m --output-format json)" || true
  STATUS="$(plutil -extract status raw - <<<"$RESULT" 2>/dev/null || true)"
  if [[ "$STATUS" != "Accepted" ]]; then
    echo "notarization failed (${STATUS:-no result}): $RESULT" >&2
    SUB_ID="$(plutil -extract id raw - <<<"$RESULT" 2>/dev/null || true)"
    [[ -z "$SUB_ID" ]] || xcrun notarytool log "$SUB_ID" --keychain-profile "$NOTARY" >&2 || true
    exit 1
  fi
  # The ticket can take a moment to reach Apple's CDN after Accepted.
  for attempt in 1 2 3; do
    xcrun stapler staple "$APP" && break
    [[ $attempt -lt 3 ]] || { echo "stapling failed" >&2; exit 1; }
    sleep 10
  done
  xcrun stapler validate "$APP"
  codesign --verify --strict "$APP"
  spctl --assess --type execute -vv "$APP" || { echo "Gatekeeper rejected $APP" >&2; exit 1; }
fi

echo "$APP"
