#!/usr/bin/env bash
# P0 exit demo, fully local: Postgres (docker) + backend (dev mode) + iroh relay + two CLI devices.
#
#   login both -> A genesis -> B pairs (code, SAS confirmed on both) -> clip A -> B
#   -> 256 MiB file A -> B (hash verified), once direct/LAN and once with --force-relay.
#
# Asserts: clip latency < CLIP_MAX_S (default 5) and path (via) Lan in the direct round, Relay when forced.
# Needs: docker, cargo (rustup), python3. Honors CARGO_TARGET_DIR, MIB (default 256), KEEP=1.
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

log() { printf '[e2e] %s\n' "$*"; }
fail() { printf '[e2e] FAIL: %s\n' "$*" >&2; exit 1; }

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

# Waits for `grep -q PATTERN FILE` (default 60 s).
wait_for() {
  local pattern="$1" file="$2" secs="${3:-60}" i=0
  until grep -q "$pattern" "$file" 2>/dev/null; do
    sleep 0.2; i=$((i + 1))
    [[ $i -lt $((secs * 5)) ]] || fail "timeout waiting for '$pattern' in $file"
  done
}

# Last JSON line of FILE that contains "event":"NAME", field FIELD.
jfield() {
  python3 -I - "$1" "$2" "$3" <<'PY'
import json, sys
path, ev, field = sys.argv[1:4]
val = None
for line in open(path):
    try:
        o = json.loads(line)
    except ValueError:
        continue
    if o.get("event") == ev and field in o:
        val = o[field]
print("" if val is None else val)
PY
}

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

log "relay"
"$SPIKE" relay "$(free_port)" >"$WORK/relay.log" 2>&1 &
PIDS+=($!)
wait_for '^RELAY http' "$WORK/relay.log" 20
RELAY="$(sed -n 's/^RELAY //p' "$WORK/relay.log" | head -1)"
log "api=$API relay=$RELAY"

# cli NAME [--force-relay] ARGS... (global flags first, then the subcommand)
EXTRA=()
cli() {
  local n="$1"; shift
  "$CLI" --data "$WORK/$n" --api "$API" --relay "$RELAY" --insecure-http --name "dev$n" \
    ${EXTRA[@]+"${EXTRA[@]}"} "$@"
}

otp_for() { # newest dev OTP logged for the address
  sed 's/\x1b\[[0-9;]*m//g' "$WORK/backend.log" | grep "dev otp" | grep "to=\"$1\"" | tail -1 \
    | sed -n 's/.*code="\([0-9]*\)".*/\1/p'
}

login() {
  local n="$1" email="$2"
  cli "$n" login --email "$email" --send-only >/dev/null
  sleep 0.5
  local code; code="$(otp_for "$email")"
  [[ -n "$code" ]] || fail "no OTP in backend log for $n"
  cli "$n" login --email "$email" --code "$code" | tee "$WORK/login-$n.json"
}

EMAIL="e2e-$$@example.test"
log "login A (genesis) and B"
login A "$EMAIL"
grep -q '"account":"created"' "$WORK/login-A.json" || fail "A should create the account"
login B "$EMAIL"
grep -q '"account":"needs_pairing"' "$WORK/login-B.json" || fail "B should need pairing"

log "pairing: B offers a code, A joins (SAS confirmed on both)"
cli B pair offer --yes >"$WORK/pair-B.out" 2>"$WORK/pair-B.err" &
BPAIR=$!
PIDS+=($BPAIR)
wait_for '"event":"offer"' "$WORK/pair-B.out" 30
CODE="$(jfield "$WORK/pair-B.out" offer code)"
[[ -n "$CODE" ]] || fail "no pairing code"
cli A pair join "$CODE" --yes >"$WORK/pair-A.out"
wait "$BPAIR" || fail "B pair offer failed: $(cat "$WORK/pair-B.err")"
SAS_A="$(jfield "$WORK/pair-A.out" sas words)"
SAS_B="$(jfield "$WORK/pair-B.out" sas words)"
[[ -n "$SAS_A" && "$SAS_A" == "$SAS_B" ]] || fail "SAS differs: '$SAS_A' vs '$SAS_B'"
grep -q '"event":"paired"' "$WORK/pair-A.out" || fail "A did not report paired"
grep -q '"event":"paired"' "$WORK/pair-B.out" || fail "B did not report paired"
log "paired, SAS words: $SAS_A"

B_NODE="$(jfield "$WORK/login-B.json" logged_in node_id)"

CLIP_MAX_S="${CLIP_MAX_S:-5}"

run_round() { # label expected-via (Lan | Relay)
  local label="$1" want_via="$2" dir="$WORK/round-$1"
  mkdir -p "$dir"

  log "[$label] clipboard A -> B"
  cli B clip watch --json --count 1 --timeout 90 >"$dir/watch.out" 2>"$dir/watch.err" &
  local w=$!; PIDS+=($w)
  wait_for '"event":"ready"' "$dir/watch.out" 60
  cli A clip put --text "hello from A ($label)" --wait 60 >"$dir/put.out"
  wait "$w" || fail "[$label] clip watch failed: $(cat "$dir/watch.err")"
  [[ "$(jfield "$dir/watch.out" clip text)" == "hello from A ($label)" ]] || fail "[$label] clip text mismatch"
  # Delivery latency: first publish attempt (A, after process start-up, login and presence settle)
  # until B reports the clip. Both are wall-clock ms on this machine.
  local p0 r1 lat settle
  p0="$(jfield "$dir/put.out" published first_publish_ms)"
  r1="$(jfield "$dir/watch.out" clip ts_ms)"
  settle="$(jfield "$dir/put.out" published settle_ms)"
  [[ -n "$p0" && -n "$r1" ]] || fail "[$label] missing clip timestamps"
  lat="$(python3 -c "print(round(($r1-$p0)/1000, 2))")"
  printf '%s clip A->B latency_s=%s (A presence settle_ms=%s, attempts=%s)\n' "$label" "$lat" "$settle" \
    "$(jfield "$dir/put.out" published attempts)" | tee -a "$RESULTS"
  python3 -c "import sys; sys.exit(0 if $lat < $CLIP_MAX_S else 1)" \
    || fail "[$label] clip latency ${lat}s exceeds ${CLIP_MAX_S}s"

  log "[$label] ${MIB} MiB file A -> B"
  local src="$dir/src.bin"
  dd if=/dev/urandom of="$src" bs=1048576 count="$MIB" 2>/dev/null
  cli B fetch --out "$dir/recv" --timeout 900 >"$dir/fetch.out" 2>"$dir/fetch.err" &
  local f=$!; PIDS+=($f)
  wait_for '"event":"ready"' "$dir/fetch.out" 60
  cli A send "devB" "$src" --wait 60 >"$dir/send.out"
  wait "$f" || fail "[$label] fetch failed: $(cat "$dir/fetch.err")"
  local got; got="$(jfield "$dir/fetch.out" fetched path)"
  [[ -f "$got" ]] || fail "[$label] received file missing"
  cmp "$src" "$got" || fail "[$label] received file differs"
  local want; want="$(shasum -a 256 "$src" | cut -d' ' -f1)"
  [[ "$(shasum -a 256 "$got" | cut -d' ' -f1)" == "$want" ]] || fail "[$label] sha256 mismatch"
  printf '%s file %s MiB sha256=%s send_secs=%s send_mib_s=%s recv_secs=%s recv_mib_s=%s via=%s blake3=%s\n' \
    "$label" "$MIB" "${want:0:16}" "$(jfield "$dir/send.out" sent secs)" "$(jfield "$dir/send.out" sent mib_s)" \
    "$(jfield "$dir/fetch.out" fetched secs)" "$(jfield "$dir/fetch.out" fetched mib_s)" \
    "$(jfield "$dir/send.out" sent via)" "$(jfield "$dir/fetch.out" fetched blake3)" | tee -a "$RESULTS"
  local via; via="$(jfield "$dir/send.out" sent via)"
  [[ "$via" == "$want_via" ]] || fail "[$label] expected via=$want_via, got '$via'"
  rm -rf "$src" "$dir/recv"
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
