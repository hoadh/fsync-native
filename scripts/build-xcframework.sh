#!/usr/bin/env bash
# Builds the universal (arm64 + x86_64) macOS static library for fsync-ffi, wraps it as an
# xcframework and places it, with the generated Swift, into the FsyncCoreKit SwiftPM package.
#
#   scripts/build-xcframework.sh
#
# Needs: rustup, Xcode command line tools (lipo, xcodebuild). Honors CARGO_TARGET_DIR.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$ROOT/apps/macos/Packages/FsyncCoreKit"
HEADERS="$ROOT/build/ffi-headers"
BUILD_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
TRIPLES=(aarch64-apple-darwin x86_64-apple-darwin)
MIN_MACOS="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
export MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS"

cd "$ROOT"
rustup target add "${TRIPLES[@]}" >/dev/null

"$ROOT/scripts/gen-bindings.sh" "$PKG/Sources/FsyncCoreKit" "$HEADERS"

LIBS=()
for t in "${TRIPLES[@]}"; do
  cargo build -p fsync-ffi --release --target "$t"
  # Shipped builds must never carry the test-only cargo features.
  if cargo tree -e normal,build,features -p fsync-ffi --target "$t" | grep -E 'test-relay-ca|test-hooks'; then
    echo "error: test-only feature enabled in the fsync-ffi release build" >&2
    exit 1
  fi
  LIBS+=("$BUILD_DIR/$t/release/libfsync_ffi.a")
done

UNIVERSAL="$ROOT/build/universal"
rm -rf "$UNIVERSAL"
mkdir -p "$UNIVERSAL"
lipo -create "${LIBS[@]}" -output "$UNIVERSAL/libfsync_ffi.a"
lipo -info "$UNIVERSAL/libfsync_ffi.a"

OUT="$PKG/FsyncFFI.xcframework"
rm -rf "$OUT"
xcodebuild -create-xcframework \
  -library "$UNIVERSAL/libfsync_ffi.a" -headers "$HEADERS" \
  -output "$OUT"
echo "xcframework: $OUT"
