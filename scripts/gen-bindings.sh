#!/usr/bin/env bash
# Generates the Swift bindings for fsync-ffi (UniFFI library mode).
#
#   scripts/gen-bindings.sh [swift-out-dir] [headers-out-dir]
#
# Defaults: Swift sources go to the FsyncCoreKit package, C header + module map to
# build/ffi-headers (used by build-xcframework.sh). Honors CARGO_TARGET_DIR.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SWIFT_OUT="${1:-$ROOT/apps/macos/Packages/FsyncCoreKit/Sources/FsyncCoreKit}"
HEADERS_OUT="${2:-$ROOT/build/ffi-headers}"
BUILD_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

cd "$ROOT"
# The dylib carries the UniFFI metadata that library mode reads.
cargo build -p fsync-ffi --release
cargo build -p fsync-ffi --release --features bindgen --bin uniffi-bindgen

LIB="$BUILD_DIR/release/libfsync_ffi.dylib"
[[ -f "$LIB" ]] || { echo "missing $LIB" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
"$BUILD_DIR/release/uniffi-bindgen" generate --library "$LIB" --language swift --out-dir "$TMP"

mkdir -p "$SWIFT_OUT" "$HEADERS_OUT"
cp "$TMP/fsync_ffi.swift" "$SWIFT_OUT/fsync_ffi.swift"
cp "$TMP/fsync_ffiFFI.h" "$HEADERS_OUT/fsync_ffiFFI.h"
# SwiftPM / xcframework headers directories need the file to be called module.modulemap.
cp "$TMP/fsync_ffiFFI.modulemap" "$HEADERS_OUT/module.modulemap"
echo "swift: $SWIFT_OUT/fsync_ffi.swift"
echo "headers: $HEADERS_OUT"
