#!/usr/bin/env bash
# Runs INSIDE the release image (started by build-release.sh): builds the release binary and
# turns it into .deb, .rpm and AppImage under dist/. Not meant to be run on the host.
# Expects: repo at /src (read-only), /src/apps/linux/dist writable, CARGO_TARGET_DIR set,
# SOURCE_DATE_EPOCH exported by the caller (stable timestamps in the packages).
set -euo pipefail
cd /src/apps/linux
dist=/src/apps/linux/dist
stage=$dist/stage
mkdir -p "$stage"

# Nothing in the environment may inject features or cfgs behind the guard's back.
[[ -z "${RUSTFLAGS:-}${CARGO_ENCODED_RUSTFLAGS:-}${CARGO_BUILD_RUSTFLAGS:-}" ]] \
  || { echo "FAIL: RUSTFLAGS / CARGO_*RUSTFLAGS is set" >&2; exit 1; }
for cfg in "$CARGO_HOME/config.toml" "$CARGO_HOME/config" /src/.cargo/config.toml \
           /src/apps/linux/.cargo/config.toml /src/apps/linux/.cargo/config; do
  [[ ! -e "$cfg" ]] || { echo "FAIL: cargo config $cfg could change the build" >&2; exit 1; }
done

cargo build --release --locked -p fsync-linux
bin="$CARGO_TARGET_DIR/release/fsync-linux"

# One stripped copy feeds all three formats (asset paths in app/Cargo.toml point at stage/).
install -m 755 "$bin" "$stage/fsync-linux"
strip --strip-unneeded "$stage/fsync-linux"

# The artifacts must be the clean build: assert on the stripped file we are about to ship.
info=$("$stage/fsync-linux" --build-info)
echo "build-info: $info"
[[ "$info" == *'"features":[]'* ]] || { echo "FAIL: --build-info lists features: $info" >&2; exit 1; }

cargo about generate --offline --locked --fail --config about.toml --manifest-path app/Cargo.toml \
  packaging/about.hbs >"$stage/THIRD-PARTY-LICENSES.txt"

version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
[[ -n "$version" ]] || { echo "no version in Cargo.toml" >&2; exit 1; }

cargo deb --manifest-path app/Cargo.toml --no-build --no-strip --output "$dist"
cargo generate-rpm -p app --target-dir "$CARGO_TARGET_DIR" -o "$dist"
packaging/appimage/build-appimage.sh "$stage" "$dist" "$version"
echo "packages built:"; ls -l "$dist"
