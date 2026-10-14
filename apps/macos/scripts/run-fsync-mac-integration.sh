#!/usr/bin/env bash
# macOS app integration run, fully local: Postgres (docker) + dev-mode backend + iroh relay + fsync-cli,
# then the FsyncMacIntegrationTests (real EngineBridge and models, CLI as the peer).
#
#   login via OTP from the backend log -> first device -> CLI offers / Mac joins (SAS) -> device online (Lan/Relay status needs phase 12)
#   -> remove -> Mac offers / CLI joins (SAS) -> recovery (resetAccountTrust)
#
# Needs: docker, cargo (rustup), python3, swift, apps/macos/Packages/FsyncCoreKit/FsyncFFI.xcframework.
# Honors CARGO_TARGET_DIR and KEEP=1 (keep the work dir). Everything it starts is stopped on exit.
# Based on tests/e2e/p0-exit-demo.sh (DRY debt: that setup block is copied here).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BUILD="${CARGO_TARGET_DIR:-$ROOT/target}"
# Relative paths would resolve against each sub-build's cwd; make it absolute.
mkdir -p "$BUILD" && BUILD="$(cd "$BUILD" && pwd)"
WORK="$(mktemp -d)"
PG_NAME="fsync-mac-it-pg-$$"
KC_SERVICE_PREFIX="app.fsync.mac.tests"
PIDS=()

log() { printf '[mac-it] %s\n' "$*"; }
fail() { printf '[mac-it] FAIL: %s\n' "$*" >&2; exit 1; }

# Removes leftover Keychain items of the test runs (services app.fsync.mac.tests.<uuid>); never the app's own.
clean_keychain() {
  local svc
  security dump-keychain 2>/dev/null | grep -o "\"svce\"<blob>=\"${KC_SERVICE_PREFIX}\.[^\"]*\"" \
    | sed 's/.*=\"\(.*\)\"/\1/' | sort -u | while read -r svc; do
      while security delete-generic-password -s "$svc" >/dev/null 2>&1; do :; done
    done || true
  return 0
}

cleanup() {
  local rc=$?
  trap - EXIT INT TERM
  # `swift test` runs the tests in an xctest child that would outlive it: stop the children first.
  for p in ${PIDS[@]+"${PIDS[@]}"}; do pkill -P "$p" 2>/dev/null || true; done
  for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done
  for p in ${PIDS[@]+"${PIDS[@]}"}; do wait "$p" 2>/dev/null || true; done
  pkill -f "$WORK" 2>/dev/null || true  # CLI peers left behind by an interrupted test run
  docker rm -f -v "$PG_NAME" >/dev/null 2>&1 || true
  clean_keychain
  if [[ $rc -ne 0 ]]; then
    log "logs kept in $WORK"
    for f in backend.log relay.log; do [[ -f "$WORK/$f" ]] && { echo "--- $f"; tail -20 "$WORK/$f"; }; done
  elif [[ -z "${KEEP:-}" ]]; then
    rm -rf "$WORK"
  fi
  exit $rc
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }

wait_for() { # PATTERN FILE [SECS]
  local pattern="$1" file="$2" secs="${3:-60}" i=0
  until grep -q "$pattern" "$file" 2>/dev/null; do
    sleep 0.2; i=$((i + 1))
    [[ $i -lt $((secs * 5)) ]] || fail "timeout waiting for '$pattern' in $file"
  done
}

[[ -d "$ROOT/apps/macos/Packages/FsyncCoreKit/FsyncFFI.xcframework" ]] \
  || fail "FsyncFFI.xcframework is missing; run scripts/build-xcframework.sh"

log "building (release): backend, fsync-cli, relay"
(cd "$ROOT" && cargo build --release -p fsync-backend -p fsync-cli)
(cd "$ROOT/spikes/iroh-discovery" && CARGO_TARGET_DIR="$BUILD/spike" cargo build --release)
CLI="$BUILD/release/fsync-cli"
BACKEND="$BUILD/release/fsync-backend"
SPIKE="$BUILD/spike/release/iroh-discovery-spike"
for b in "$CLI" "$BACKEND" "$SPIKE"; do [[ -x "$b" ]] || fail "missing binary $b"; done

log "postgres ($PG_NAME)"
docker run -d --name "$PG_NAME" -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=fsync \
  -p 127.0.0.1::5432 postgres:17 >/dev/null
PGPORT="$(docker port "$PG_NAME" 5432/tcp | head -1 | sed 's/.*://')"
for _ in $(seq 1 60); do
  docker exec "$PG_NAME" pg_isready -U postgres -d fsync >/dev/null 2>&1 && break
  sleep 0.5
done
sleep 1
docker exec "$PG_NAME" pg_isready -U postgres -d fsync >/dev/null || fail "postgres not ready"

log "backend (dev mode)"
API_PORT="$(free_port)"
API="http://127.0.0.1:$API_PORT"
DATABASE_URL="postgres://postgres:e2e@127.0.0.1:$PGPORT/fsync" BIND_ADDR="127.0.0.1:$API_PORT" \
  FSYNC_DEV_MODE=1 RUST_LOG="fsync_backend=debug,info" NO_COLOR=1 \
  "$BACKEND" >"$WORK/backend.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 100); do curl -fs "$API/health" >/dev/null 2>&1 && break; sleep 0.2; done
curl -fs "$API/health" >/dev/null || fail "backend not healthy"

log "relay"
"$SPIKE" relay "$(free_port)" >"$WORK/relay.log" 2>&1 &
PIDS+=($!)
wait_for '^RELAY http' "$WORK/relay.log" 20
RELAY="$(sed -n 's/^RELAY //p' "$WORK/relay.log" | head -1)"
log "api=$API relay=$RELAY"

export FSYNC_IT_API="$API" FSYNC_IT_RELAY="$RELAY" FSYNC_IT_BACKEND_LOG="$WORK/backend.log"
export FSYNC_IT_CLI="$CLI" FSYNC_IT_WORK="$WORK"

log "swift test (integration)"
# Background + wait, so a signal reaches the trap at once instead of after the test run.
: >"$WORK/swift-test.log"
tail -f "$WORK/swift-test.log" &
PIDS+=($!)
swift test --package-path "$ROOT/apps/macos/FsyncMac" --filter FsyncMacIntegrationTests >>"$WORK/swift-test.log" 2>&1 &
SWIFT_PID=$!
PIDS+=($SWIFT_PID)
wait "$SWIFT_PID" || { sleep 0.5; fail "swift test failed"; }
sleep 0.5
grep -Eq "Executed [1-9][0-9]* tests?, with 0 failures" "$WORK/swift-test.log" || fail "no integration tests ran"
grep -Eq "[1-9][0-9]* tests? skipped" "$WORK/swift-test.log" && fail "integration tests were skipped"

log "PASS"
