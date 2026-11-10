#!/usr/bin/env bash
# Pairing flake loop (plan 261110 phase 01): N fsync-cli <-> fsync-cli pairings against one local
# Postgres + backend (dev mode) + relay. Each run uses a fresh account and fresh data dirs:
# devA logs in (creates the account), devB logs in (needs pairing), devB offers, devA joins.
# Both are freshly started processes, so both endpoints are "just started" (the flake condition).
#
#   apps/linux/tests/e2e/pair-flake-loop.sh [RUNS]          (default 30)
#
# Env: FORCE_RELAY=1 (relay-only on both CLIs), JOIN_DELAY_S (sleep between the offer code and
# the join, default 0), RUST_LOG (default iroh=debug,fsync_core=debug), STOP_ON_FAIL=1,
# CARGO_TARGET_DIR, NO_BUILD=1. Logs: one dir per run under $OUT (printed; always kept).
# Prints a summary line `runs=N pass=P fail=F` and exits 1 when any run failed.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
BUILD="${CARGO_TARGET_DIR:-$ROOT/target}"
mkdir -p "$BUILD" && BUILD="$(cd "$BUILD" && pwd)"
RUNS="${1:-30}"

need() { command -v "$1" >/dev/null 2>&1 || { echo "[e2e] missing prerequisite: $1 ($2)" >&2; exit 2; }; }
need docker "needed for Postgres"; need python3 ""; need curl ""; need cargo "rustup"

OUT="${OUT:-$(mktemp -d /tmp/fsync-pair-loop.XXXXXX)}"
mkdir -p "$OUT"
PG_NAME="fsync-pair-loop-pg-$$"
PIDS=()
. "$ROOT/tests/e2e/lib/assert.sh"; . "$ROOT/tests/e2e/lib/cli-session.sh"; . "$HERE/lib-postgres-ready.sh"

cleanup() {
  local rc=$?
  for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done
  for p in ${PIDS[@]+"${PIDS[@]}"}; do wait "$p" 2>/dev/null || true; done
  docker rm -f -v "$PG_NAME" >/dev/null 2>&1 || true
  log "logs in $OUT"
  exit $rc
}
trap cleanup EXIT INT TERM

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }

if [[ -z "${NO_BUILD:-}" ]]; then
  log "building: backend, fsync-cli (release), relay"
  (cd "$ROOT" && cargo build --release -p fsync-backend -p fsync-cli)
  (cd "$ROOT/spikes/iroh-discovery" && CARGO_TARGET_DIR="$BUILD/spike" cargo build --release)
fi
CLI="$BUILD/release/fsync-cli"; BACKEND="$BUILD/release/fsync-backend"
SPIKE="$BUILD/spike/release/iroh-discovery-spike"
for b in "$CLI" "$BACKEND" "$SPIKE"; do [[ -x "$b" ]] || fail "missing binary $b"; done

log "postgres"
docker run -d --name "$PG_NAME" -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=fsync -p 127.0.0.1::5432 postgres:17 >/dev/null
PGPORT="$(docker port "$PG_NAME" 5432/tcp | head -1 | sed 's/.*://')"
wait_pg_ready "$PG_NAME"

WORK="$OUT"   # cli-session.sh reads $WORK/backend.log for the OTP
API_PORT="$(free_port)"; API="http://127.0.0.1:$API_PORT"
# The backend allows 20 OTP starts per IP and hour (in memory): restart it every 9 runs (2 logins each).
BACKEND_PID=""
start_backend() {
  [[ -n "$BACKEND_PID" ]] && { kill "$BACKEND_PID" 2>/dev/null || true; wait "$BACKEND_PID" 2>/dev/null || true; }
  DATABASE_URL="postgres://postgres:e2e@127.0.0.1:$PGPORT/fsync" BIND_ADDR="127.0.0.1:$API_PORT" \
    FSYNC_DEV_MODE=1 RUST_LOG="fsync_backend=debug,info" NO_COLOR=1 "$BACKEND" >>"$OUT/backend.log" 2>&1 &
  BACKEND_PID=$!; PIDS+=("$BACKEND_PID")
  for _ in $(seq 1 100); do curl -fs "$API/health" >/dev/null 2>&1 && break; sleep 0.2; done
  curl -fs "$API/health" >/dev/null || fail "backend not healthy"
}
start_backend
"$SPIKE" relay "$(free_port)" >"$OUT/relay.log" 2>&1 &
PIDS+=($!)
wait_for '^RELAY http' "$OUT/relay.log" 20
RELAY="$(sed -n 's/^RELAY //p' "$OUT/relay.log" | head -1)"
log "api=$API relay=$RELAY force_relay=${FORCE_RELAY:-0} join_delay=${JOIN_DELAY_S:-0}"
[[ -n "${FORCE_RELAY:-}" ]] && EXTRA=(--force-relay)
export RUST_LOG="${RUST_LOG:-iroh=debug,fsync_core=debug}" NO_COLOR=1

one_run() { # N: runs in a subshell; `fail` ends only this run
  local n="$1"
  WORK="$OUT/run-$n"; mkdir -p "$WORK"; ln -sf ../backend.log "$WORK/backend.log"
  local email="loop-$$-$n@example.test"
  login A "$email" >/dev/null 2>"$WORK/login-A.err"
  login B "$email" >/dev/null 2>"$WORK/login-B.err"
  cli B pair offer --yes >"$WORK/pair-B.out" 2>"$WORK/pair-B.err" &
  OFFER_PID=$!
  trap 'kill "$OFFER_PID" 2>/dev/null || true' EXIT
  wait_for '"event":"offer"' "$WORK/pair-B.out" 30
  local code; code="$(jfield "$WORK/pair-B.out" offer code)"
  [[ -n "$code" ]] || fail "no pairing code"
  [[ -n "${JOIN_DELAY_S:-}" ]] && sleep "$JOIN_DELAY_S"
  local jrc=0 orc=0
  cli A pair join "$code" --yes >"$WORK/pair-A.out" 2>"$WORK/pair-A.err" || jrc=$?
  wait "$OFFER_PID" || orc=$?
  if [[ $jrc -ne 0 || $orc -ne 0 ]]; then
    fail "join rc=$jrc ($(grep '^Error' "$WORK/pair-A.err" | tail -1)) offer rc=$orc ($(grep '^Error' "$WORK/pair-B.err" | tail -1))"
  fi
  grep -q '"event":"paired"' "$WORK/pair-A.out" || fail "devA did not report paired"
  grep -q '"event":"paired"' "$WORK/pair-B.out" || fail "devB did not report paired"
}

PASS=0; FAILN=0
for n in $(seq 1 "$RUNS"); do
  [[ $n -gt 1 && $(( (n - 1) % 9 )) -eq 0 ]] && start_backend
  if (one_run "$n") >"$OUT/run-$n.result" 2>&1; then
    PASS=$((PASS + 1)); echo "run $n PASS" | tee -a "$OUT/summary.txt"
  else
    FAILN=$((FAILN + 1)); echo "run $n FAIL: $(cat "$OUT/run-$n.result" "$OUT/run-$n"/login-*.err 2>/dev/null | grep -h 'FAIL' | tail -1)" | tee -a "$OUT/summary.txt"
    [[ -n "${STOP_ON_FAIL:-}" ]] && break
  fi
done
echo "runs=$((PASS + FAILN)) pass=$PASS fail=$FAILN force_relay=${FORCE_RELAY:-0} join_delay=${JOIN_DELAY_S:-0}" | tee -a "$OUT/summary.txt"
[[ $FAILN -eq 0 ]]
