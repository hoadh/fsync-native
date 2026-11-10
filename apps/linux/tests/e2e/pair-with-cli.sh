#!/usr/bin/env bash
# shellcheck disable=SC2034  # variables are consumed by the sourced libs
# Linux app end-to-end (phase 02): the app (offscreen Qt, debug build with the e2e-control
# feature) signs in and pairs with an fsync-cli device, driven over its IPC socket.
#
#   apps/linux/tests/e2e/pair-with-cli.sh
#
# Steps: Postgres (docker) + local relay + backend (dev mode, OTP in its log) -> CLI devA creates
# the account -> app signs in via IPC (needs pairing) -> app offers a code -> CLI joins -> SAS
# words match -> app confirms -> both list each other (app shows as "cli" on the CLI) -> second
# launch `--show` forwards and exits 0 -> offer + IPC cancel returns in < 1 s -> SIGTERM exits < 6 s.
# Needs: docker, dbus-run-session, python3, cargo + qmake6 (QMAKE), curl. Honors CARGO_TARGET_DIR,
# KEEP=1. Missing prerequisite: exit 2 with a hint.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
BUILD="${CARGO_TARGET_DIR:-$ROOT/target}"
mkdir -p "$BUILD" && BUILD="$(cd "$BUILD" && pwd)"
APP_BIN="$ROOT/apps/linux/target/debug/fsync-linux"

need() { command -v "$1" >/dev/null 2>&1 || { echo "[e2e] missing prerequisite: $1 ($2)" >&2; exit 2; }; }
need docker "needed for Postgres"; need dbus-run-session "package dbus"; need python3 ""; need curl ""
need cargo "rustup"

WORK="$(mktemp -d)"
RT="$(mktemp -d /tmp/fsync-e2e-rt.XXXXXX)"; chmod 700 "$RT" # short path: unix socket limit
PG_NAME="fsync-e2e-linux-pg-$$"
PIDS=()
. "$ROOT/tests/e2e/lib/assert.sh"; . "$ROOT/tests/e2e/lib/cli-session.sh"; . "$(dirname "${BASH_SOURCE[0]}")/lib-postgres-ready.sh"

cleanup() {
  local rc=$?
  for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done
  for p in ${PIDS[@]+"${PIDS[@]}"}; do wait "$p" 2>/dev/null || true; done
  docker rm -f -v "$PG_NAME" >/dev/null 2>&1 || true
  rm -rf "$RT"
  if [[ $rc -ne 0 ]]; then
    log "logs kept in $WORK"
    for f in backend.log relay.log app.log; do [[ -f "$WORK/$f" ]] && { echo "--- $f"; tail -25 "$WORK/$f"; }; done
  elif [[ -z "${KEEP:-}" ]]; then rm -rf "$WORK"; fi
  exit $rc
}
trap cleanup EXIT INT TERM

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }

log "building: backend, fsync-cli (release), relay, app (debug, e2e-control)"
(cd "$ROOT" && cargo build --release -p fsync-backend -p fsync-cli)
(cd "$ROOT/spikes/iroh-discovery" && CARGO_TARGET_DIR="$BUILD/spike" cargo build --release)
(cd "$ROOT/apps/linux" && QMAKE="${QMAKE:-/usr/bin/qmake6}" cargo build --features e2e-control)
CLI="$BUILD/release/fsync-cli"; BACKEND="$BUILD/release/fsync-backend"
SPIKE="$BUILD/spike/release/iroh-discovery-spike"
for b in "$CLI" "$BACKEND" "$SPIKE" "$APP_BIN"; do [[ -x "$b" ]] || fail "missing binary $b"; done
"$APP_BIN" --build-info | grep -q e2e-control || fail "app binary was not built with e2e-control"

log "postgres"
docker run -d --name "$PG_NAME" -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=fsync -p 127.0.0.1::5432 postgres:17 >/dev/null
PGPORT="$(docker port "$PG_NAME" 5432/tcp | head -1 | sed 's/.*://')"
wait_pg_ready "$PG_NAME"

log "backend (dev mode) + relay"
API_PORT="$(free_port)"; API="http://127.0.0.1:$API_PORT"
DATABASE_URL="postgres://postgres:e2e@127.0.0.1:$PGPORT/fsync" BIND_ADDR="127.0.0.1:$API_PORT" \
  FSYNC_DEV_MODE=1 RUST_LOG="fsync_backend=debug,info" NO_COLOR=1 "$BACKEND" >"$WORK/backend.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 100); do curl -fs "$API/health" >/dev/null 2>&1 && break; sleep 0.2; done
curl -fs "$API/health" >/dev/null || fail "backend not healthy"
"$SPIKE" relay "$(free_port)" >"$WORK/relay.log" 2>&1 &
PIDS+=($!)
wait_for '^RELAY http' "$WORK/relay.log" 20
RELAY="$(sed -n 's/^RELAY //p' "$WORK/relay.log" | head -1)"
log "api=$API relay=$RELAY"

# ipc JSON: one request line to the app's socket; prints the reply line.
ipc() {
  local sock; sock="$(ls "$RT"/fsync/*.sock 2>/dev/null | head -1)"
  [[ -S "$sock" ]] || { echo '{"ok":false,"error":"no socket"}'; return 1; }
  python3 -I - "$sock" "$1" <<'PY'
import socket, sys
s = socket.socket(socket.AF_UNIX); s.settimeout(8); s.connect(sys.argv[1])
s.sendall(sys.argv[2].encode() + b"\n")
buf = b""
while not buf.endswith(b"\n"):
    c = s.recv(65536)
    if not c: break
    buf += c
sys.stdout.write(buf.decode())
PY
}
# status_field PATH (dotted): value from the app's status reply.
status_field() {
  ipc '{"cmd":"status"}' | python3 -I -c '
import json, sys
o = json.load(sys.stdin)["data"]
for k in sys.argv[1].split("."):
    o = o[k]
print(o)' "$1"
}
wait_status() { # PATH VALUE [SECS]
  local i
  for i in $(seq 1 $((${3:-30} * 5))); do
    [[ "$(status_field "$1" 2>/dev/null || true)" == "$2" ]] && return 0; sleep 0.2
  done
  fail "app status $1 never became '$2' (now '$(status_field "$1" 2>/dev/null || echo ?)', message: '$(status_field message 2>/dev/null || true)')"
}
app_pid() { pgrep -f "^$APP_BIN --data-dir $WORK/appdata" | head -1 || true; }

EMAIL="linux-e2e-$$@example.test"
log "devA creates the account"
login A "$EMAIL"
grep -q '"account":"created"' "$WORK/login-A.json" || fail "devA should create the account"

log "starting the app (offscreen, private session bus, file secrets)"
mkdir -p "$WORK/apphome" "$WORK/appcfg" "$WORK/appdata"
(
  export HOME="$WORK/apphome" XDG_CONFIG_HOME="$WORK/appcfg" XDG_RUNTIME_DIR="$RT" \
    FSYNC_SECRETS=file QT_QPA_PLATFORM=offscreen FSYNC_API="$API" FSYNC_RELAY="$RELAY" \
    FSYNC_DEVICE_NAME=devApp FSYNC_LOG=info
  # Phase 03: the app runs a clipboard backend. This test must never see the user's real
  # session (clipboard included): no display at all, so the backend is Manual.
  # (The GTK platform theme aborts without a display, hence the generic theme.)
  unset DISPLAY WAYLAND_DISPLAY SWAYSOCK HYPRLAND_INSTANCE_SIGNATURE
  export QT_QPA_PLATFORMTHEME=generic
  exec dbus-run-session -- "$APP_BIN" --data-dir "$WORK/appdata"
) >"$WORK/app.log" 2>&1 &
APP_RUNNER=$!; PIDS+=("$APP_RUNNER")
for _ in $(seq 1 100); do ls "$RT"/fsync/*.sock >/dev/null 2>&1 && break; sleep 0.2; done
wait_status state signed-out 30
log "app up, signed out; secrets backend: $(cat "$WORK/appdata/secrets-backend")"
assert_eq "$(stat -c %a "$WORK/appdata/secrets.json")" 600 "secrets.json mode"

ipc '{"cmd":"clip-send"}' | grep -q '"error":"signed-out"' || fail "clip-send must answer signed-out before sign-in"

log "app signs in over IPC"
ipc "{\"cmd\":\"login\",\"email\":\"$EMAIL\"}" | grep -q '"ok":true' || fail "login rejected"
wait_status state awaiting-code 20
CODE=""
for _ in $(seq 1 20); do CODE="$(otp_for "$EMAIL" | tail -1)"; [[ -n "$CODE" ]] && break; sleep 0.3; done
[[ -n "$CODE" ]] || fail "no OTP"
ipc "{\"cmd\":\"login\",\"email\":\"$EMAIL\",\"code\":\"$CODE\"}" | grep -q '"ok":true' || fail "verify rejected"
wait_status state needs-pairing 40
log "app signed in, needs pairing"

log "app offers a code, devA joins"
ipc '{"cmd":"pair-offer"}' | grep -q '"ok":true' || fail "pair-offer rejected"
PCODE=""
for _ in $(seq 1 100); do PCODE="$(status_field pairing.code)"; [[ "$PCODE" =~ ^[0-9]{6}$ ]] && break; sleep 0.2; done
[[ "$PCODE" =~ ^[0-9]{6}$ ]] || fail "no pairing code on the app (got '$PCODE')"
cli A pair join "$PCODE" --yes >"$WORK/pair-A.out" 2>"$WORK/pair-A.err" &
JOIN=$!; PIDS+=("$JOIN")
wait_status pairing.phase confirm 60
APP_SAS="$(status_field pairing.sas)"; REQ="$(status_field pairing.request_id)"
wait_for '"event":"sas"' "$WORK/pair-A.out" 30
CLI_SAS="$(jfield "$WORK/pair-A.out" sas words)"
[[ -n "$APP_SAS" && "$APP_SAS" == "$CLI_SAS" ]] || fail "SAS differs: app '$APP_SAS' vs CLI '$CLI_SAS'"
log "SAS words match: $APP_SAS"
ipc "{\"cmd\":\"pair-confirm\",\"request_id\":\"$REQ\",\"yes\":true}" | grep -q '"ok":true' || fail "pair-confirm rejected"
wait "$JOIN" || fail "devA pair join failed: $(cat "$WORK/pair-A.err")"
grep -q '"event":"paired"' "$WORK/pair-A.out" || fail "devA did not report paired"
wait_status state ready 60
log "paired; app is ready"

log "device lists"
wait_status pairing.phase done 10
APP_DEVICES="$(status_field devices)"
echo "$APP_DEVICES" | grep -q "devA" || fail "app does not list devA: $APP_DEVICES"
cli A devices --json >"$WORK/devices-A.json"
python3 -I - "$WORK/devices-A.json" <<'PY' || fail "devA does not list the app as a cli device"
import json, sys
rows = json.loads(open(sys.argv[1]).read().strip().splitlines()[-1])
app = [r for r in rows if r["name"] == "devApp"]
sys.exit(0 if app and app[0]["platform"] == "cli" else 1)
PY
log "devA lists devApp as platform cli; app lists devA"

log "second launch --show forwards and exits 0"
T0=$(date +%s.%N)
(HOME="$WORK/apphome" XDG_RUNTIME_DIR="$RT" QT_QPA_PLATFORM=offscreen "$APP_BIN" --data-dir "$WORK/appdata" --show) \
  || fail "second launch failed"
assert_lt "$(python3 -c "print($(date +%s.%N)-$T0)")" 1 "second launch --show duration (s)"
[[ "$(app_pid)" =~ ^[0-9]+$ ]] || fail "first instance died"

# Ready on the Manual backend (no display): the command opens the QuickClipWindow and answers ok.
ipc '{"cmd":"clip-paste"}' | grep -q '"ok":true' || fail "clip-paste while ready should be accepted (manual backend, quick window)"

log "offer + cancel is fast"
ipc '{"cmd":"pair-offer"}' | grep -q '"ok":true' || fail "second pair-offer rejected"
wait_status pairing.phase offering 20
T0=$(date +%s.%N)
ipc '{"cmd":"cancel"}' | grep -q '"ok":true' || fail "cancel rejected"
wait_status pairing.phase idle 5
assert_lt "$(python3 -c "print($(date +%s.%N)-$T0)")" 1 "cancel duration (s)"

log "SIGTERM exits within the 6 s shutdown deadline"
ipc '{"cmd":"pair-offer"}' >/dev/null || true
APP_PID="$(app_pid)"; [[ "$APP_PID" =~ ^[0-9]+$ ]] || fail "cannot find the app process"
T0=$(date +%s.%N)
kill -TERM "$APP_PID"
for _ in $(seq 1 80); do kill -0 "$APP_PID" 2>/dev/null || break; sleep 0.1; done
! kill -0 "$APP_PID" 2>/dev/null || fail "app still running 8 s after SIGTERM"
assert_lt "$(python3 -c "print($(date +%s.%N)-$T0)")" 6.5 "quit duration (s)"
[[ ! -S "$(ls "$RT"/fsync/*.sock 2>/dev/null | head -1)" ]] || fail "socket left behind"

log "PASS"
