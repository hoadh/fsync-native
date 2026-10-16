#!/usr/bin/env bash
# P0 exit demo, fully local: Postgres (docker) + backend (dev mode) + iroh relay + two CLI devices.
#
#   login both -> A genesis -> B pairs (code, SAS confirmed on both) -> clip A -> B
#   -> 256 MiB file A -> B (hash verified), once direct/LAN and once with --force-relay.
#
# Asserts: clip latency < CLIP_MAX_S (default 5) and path (via) Lan in the direct round, Relay when forced.
# Needs: docker, cargo (rustup), python3. Honors CARGO_TARGET_DIR, MIB (default 256), KEEP=1,
# E2E_RELAY_URL (use a deployed relay instead of the local one).
# Everything it starts is stopped on exit.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD="${CARGO_TARGET_DIR:-$ROOT/target}"
# Relative paths would resolve against each sub-build's cwd; make it absolute.
mkdir -p "$BUILD" && BUILD="$(cd "$BUILD" && pwd)"
MIB="${MIB:-256}"
WORK="$(mktemp -d)"
PG_NAME="fsync-e2e-pg-$$"
PIDS=()
RESULTS="$WORK/results.txt"
: >"$RESULTS"

. "$(dirname "${BASH_SOURCE[0]}")/lib/assert.sh"; . "$(dirname "${BASH_SOURCE[0]}")/lib/cli-session.sh"

cleanup() {
  local rc=$?
  for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done
  for p in ${PIDS[@]+"${PIDS[@]}"}; do wait "$p" 2>/dev/null || true; done
  docker rm -f -v "$PG_NAME" >/dev/null 2>&1 || true
  if [[ $rc -ne 0 ]]; then
    log "logs kept in $WORK"
    for f in backend.log relay.log; do [[ -f "$WORK/$f" ]] && { echo "--- $f"; tail -20 "$WORK/$f"; }; done
  elif [[ -z "${KEEP:-}" ]]; then
    rm -rf "$WORK"
  fi
  exit $rc
}
trap cleanup EXIT INT TERM

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }


log "building (release): backend, fsync-cli, relay"
(cd "$ROOT" && cargo build --release -p fsync-backend -p fsync-cli)
(cd "$ROOT/spikes/iroh-discovery" && CARGO_TARGET_DIR="$BUILD/spike" cargo build --release)
CLI="$BUILD/release/fsync-cli"
BACKEND="$BUILD/release/fsync-backend"
SPIKE="$BUILD/spike/release/iroh-discovery-spike"
for b in "$CLI" "$BACKEND" "$SPIKE"; do [[ -x "$b" ]] || fail "missing binary $b"; done

log "postgres"
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

if [[ -n "${E2E_RELAY_URL:-}" ]]; then
  # Exercise a deployed relay (e.g. https://relay-sg-fsync.10z.one) instead of the local spike.
  RELAY="$E2E_RELAY_URL"
else
  log "relay"
  "$SPIKE" relay "$(free_port)" >"$WORK/relay.log" 2>&1 &
  PIDS+=($!)
  wait_for '^RELAY http' "$WORK/relay.log" 20
  RELAY="$(sed -n 's/^RELAY //p' "$WORK/relay.log" | head -1)"
fi
log "api=$API relay=$RELAY"

EMAIL="e2e-$$@example.test"
log "login A (genesis) and B"
login A "$EMAIL"
grep -q '"account":"created"' "$WORK/login-A.json" || fail "A should create the account"
login B "$EMAIL"
grep -q '"account":"needs_pairing"' "$WORK/login-B.json" || fail "B should need pairing"

log "pairing: B offers a code, A joins (SAS confirmed on both)"
pair B A

# shellcheck disable=SC2034
B_NODE="$(jfield "$WORK/login-B.json" logged_in node_id)"

export CLIP_MAX_S="${CLIP_MAX_S:-5}"

run_round() { # label expected-via (Lan | Relay)
  clip_round A B "$1"
  file_round A B "$1"
  [[ "$FILE_VIA" == "$2" ]] || fail "[$1] expected via=$2, got '$FILE_VIA'"
}


EXTRA=()
run_round direct Lan
EXTRA=(--force-relay)
run_round relay-only Relay

log "devices as seen by A"
EXTRA=()
cli A devices --json | tee "$WORK/devices.json"

log "PASS"
echo "---- results"
cat "$RESULTS"
