#!/usr/bin/env bash
# shellcheck disable=SC2034  # variables are consumed by the sourced libs
# Phase 06 cross-OS test. The Linux app (debug, e2e-control build) runs in an isolated session on
# this host (nested HEADLESS sway, own XDG dirs, FSYNC_SECRETS=file, --data-dir; the user's real
# clipboard, keyring, desktop and running fsync-linux are never touched).
#
#   cross-os-with-windows.sh --local-only    X1: Linux app <-> fsync-cli on THIS host (no Windows, no Rig)
#   cross-os-with-windows.sh                 X1 + X2 (+ X5 with X5=1): Linux app <-> fsync-cli on waso-vm
#
# X1: text both ways, file push both ways (sha256 equal), latency per leg <= 5 s, no echo/duplicate
# clips. Result: docs/test-results/linux-cross-os-<yymmdd-hhmm>.json (override: RESULT_FILE).
#
# Full mode (Windows legs; needs Rig on the worker, the branch pushed, key-based ssh from this host to it):
#   - backend + relay listen on FIXED loopback ports (X_API_PORT, X_RELAY_PORT; defaults 18080, 18090).
#     This host opens a REVERSE tunnel for the whole run (ssh -N -R 127.0.0.1:P:127.0.0.1:P <target>;
#     the target is read from ~/.claude/rig-machines.json, this host has no sshd), so waso-vm's own
#     127.0.0.1:<port> reaches them and both peers use the same http://127.0.0.1:<port> URLs
#     (plan: Option A, no http override). Health through the tunnel is checked before every run.
#   - split `remote run`s (one blocking call each, output only at the end): build, pair, then one run
#     per leg. No cross-machine barrier: the sender retries until the deadline (see leg_* below).
#   - the OTP for the Windows device comes from THIS host's backend log (OTP is bound to email +
#     code only, not to the client that started login) and is passed as `-Otp <code>`.
# Env: WIN_MACHINE (default waso-vm), REF (default HEAD; must be pushed),
#      X5=1 (relay-forced variant, CLI side `--force-relay`, text + 8 MiB file), SKIP_BUILD=1, KEEP=1,
#      CARGO_TARGET_DIR, REMOTE_JS (default ~/.claude/skills/remote/scripts/remote.cjs).
# Needs: docker, dbus-run-session, python3, cargo + qmake6, curl, sway, wl-clipboard, Xvfb, xclip.
set -euo pipefail

LOCAL_ONLY=0
case "${1:-}" in --local-only) LOCAL_ONLY=1 ;; "") ;; *) echo "usage: $0 [--local-only]" >&2; exit 2 ;; esac

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
BUILD="${CARGO_TARGET_DIR:-$ROOT/target}"
mkdir -p "$BUILD" && BUILD="$(cd "$BUILD" && pwd)"
LIN="$ROOT/apps/linux"
APP_BIN="$LIN/target/debug/fsync-linux"
STAMP="$(date +%y%m%d-%H%M)"
RESULT_FILE="${RESULT_FILE:-$ROOT/docs/test-results/linux-cross-os-$STAMP.json}"
MAX_S=5

. "$HERE/lib-headless-sessions.sh"
hs_prereq
need() { command -v "$1" >/dev/null 2>&1 || { echo "[e2e] missing prerequisite: $1 ($2)" >&2; exit 2; }; }
need docker "needed for Postgres"; need python3 ""; need curl ""; need cargo "rustup"; need sha256sum "coreutils"
[[ $LOCAL_ONLY -eq 1 ]] || need node "runs the Rig remote script"

WORK="$(mktemp -d)"
PG_NAME="fsync-e2e-linux-xos-pg-$$"
PIDS=()
LEGS="$WORK/legs.tsv"; : >"$LEGS"   # id <TAB> direction <TAB> kind <TAB> latency_ms <TAB> sha_ok <TAB> note
. "$ROOT/tests/e2e/lib/assert.sh"; . "$ROOT/tests/e2e/lib/cli-session.sh"
. "$HERE/lib-postgres-ready.sh"; . "$HERE/lib-app-instance.sh"

cleanup() {
  local rc=$?
  app_cleanup
  for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done
  for p in ${PIDS[@]+"${PIDS[@]}"}; do wait "$p" 2>/dev/null || true; done
  hs_stop_all
  docker rm -f -v "$PG_NAME" >/dev/null 2>&1 || true
  if [[ $rc -ne 0 ]]; then
    log "logs kept in $WORK"
    for f in backend.log relay.log app-L1.log tunnel.log; do [[ -f "$WORK/$f" ]] && { echo "--- $f"; tail -25 "$WORK/$f"; }; done
  elif [[ -z "${KEEP:-}" ]]; then rm -rf "$WORK"; fi
  exit $rc
}
trap cleanup EXIT INT TERM

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
now_ms() { date +%s%3N; }
sha() { sha256sum | cut -d' ' -f1; }
make_file() { mkdir -p "$(dirname "$1")"; head -c "$2" /dev/urandom >"$1"; }
# leg ID DIRECTION KIND LATENCY_MS SHA_OK NOTE: records the leg; the latency gate is applied here.
leg() {
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" "${6:-}" >>"$LEGS"
  log "leg $1 ($2, $3): ${4} ms, sha_ok=$5 ${6:-}"
  [[ "$4" -le $((MAX_S * 1000)) ]] || fail "leg $1 latency $4 ms is over ${MAX_S} s"
}

# ---- build ----------------------------------------------------------------------------------
if [[ -z "${SKIP_BUILD:-}" ]]; then
  log "building: backend, fsync-cli (release), relay, app (debug, e2e-control)"
  (cd "$ROOT" && cargo build --release -p fsync-backend -p fsync-cli)
  (cd "$ROOT/spikes/iroh-discovery" && CARGO_TARGET_DIR="$BUILD/spike" cargo build --release)
  (cd "$LIN" && QMAKE="${QMAKE:-/usr/bin/qmake6}" cargo build --features e2e-control)
fi
CLI="$BUILD/release/fsync-cli"; BACKEND="$BUILD/release/fsync-backend"; SPIKE="$BUILD/spike/release/iroh-discovery-spike"
for b in "$CLI" "$BACKEND" "$SPIKE" "$APP_BIN"; do [[ -x "$b" ]] || fail "missing binary $b"; done
"$APP_BIN" --build-info | grep -q e2e-control || fail "app binary was not built with e2e-control"

# ---- services: postgres, backend (dev mode), relay -----------------------------------------
log "postgres, backend (dev mode), relay"
docker run -d --name "$PG_NAME" -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=fsync -p 127.0.0.1::5432 postgres:17 >/dev/null
PGPORT="$(docker port "$PG_NAME" 5432/tcp | head -1 | sed 's/.*://')"
wait_pg_ready "$PG_NAME"
X_API_PORT="${X_API_PORT:-18080}"; X_RELAY_PORT="${X_RELAY_PORT:-18090}"
if [[ $LOCAL_ONLY -eq 1 ]]; then API_PORT="$(free_port)"; RELAY_PORT="$(free_port)"
else API_PORT="$X_API_PORT"; RELAY_PORT="$X_RELAY_PORT"; fi
API="http://127.0.0.1:$API_PORT"
DATABASE_URL="postgres://postgres:e2e@127.0.0.1:$PGPORT/fsync" BIND_ADDR="127.0.0.1:$API_PORT" \
  FSYNC_DEV_MODE=1 RUST_LOG="fsync_backend=debug,info" NO_COLOR=1 "$BACKEND" >"$WORK/backend.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 100); do curl -fs "$API/health" >/dev/null 2>&1 && break; sleep 0.2; done
curl -fs "$API/health" >/dev/null || fail "backend not healthy"
"$SPIKE" relay "$RELAY_PORT" >"$WORK/relay.log" 2>&1 &
PIDS+=($!)
wait_for '^RELAY http' "$WORK/relay.log" 20
RELAY="$(sed -n 's/^RELAY //p' "$WORK/relay.log" | head -1)"
[[ $LOCAL_ONLY -eq 1 || "$RELAY" == "http://127.0.0.1:$RELAY_PORT"* ]] || fail "relay URL '$RELAY' is not on the forwarded loopback port $RELAY_PORT"

# ---- reverse tunnel to the worker (full mode only; killed with the other PIDS in the trap) -----
TUNNEL_PID=""
if [[ $LOCAL_ONLY -eq 0 ]]; then
  . "$HERE/lib-cross-os-windows-legs.sh"
  log "reverse tunnel: $WIN_SSH 127.0.0.1:{$API_PORT,$RELAY_PORT} -> this host"
  setsid ssh -N -o BatchMode=yes -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 \
    -R "127.0.0.1:$API_PORT:127.0.0.1:$API_PORT" -R "127.0.0.1:$RELAY_PORT:127.0.0.1:$RELAY_PORT" "$WIN_SSH" \
    >"$WORK/tunnel.log" 2>&1 &
  TUNNEL_PID=$!; PIDS+=("$TUNNEL_PID")
  sleep 3; kill -0 "$TUNNEL_PID" 2>/dev/null || fail "ssh reverse tunnel did not start: $(tail -3 "$WORK/tunnel.log")"
fi

# ---- headless sway for the app ---------------------------------------------------------------
log "headless sway (L1)"
hs_start_sway A
SA_RT="$HS_A_RT"; SA_WL="$HS_A_WL"
wl_a() { env -u DISPLAY XDG_RUNTIME_DIR="$SA_RT" WAYLAND_DISPLAY="$SA_WL" "$@"; }

# Clips seen by devA's watch, by sender name. Only the app's own sends count for devL1.
clip_count() { grep '"event":"clip"' "$WATCH_OUT" | grep -c "\"from_name\":\"$1\"" || true; }
last_clip() { python3 -I - "$WATCH_OUT" "$1" "$2" <<'PY'
import json, sys
last = None
for line in open(sys.argv[1]):
    try: o = json.loads(line)
    except ValueError: continue
    if o.get("event") == "clip" and o.get("from_name") == sys.argv[2]: last = o
print("" if last is None else last.get(sys.argv[3], ""))
PY
}
wait_clips() { # SENDER COUNT [SECS]
  local i; for i in $(seq 1 $((${3:-15} * 5))); do [[ "$(clip_count "$1")" -ge "$2" ]] && return 0; sleep 0.2; done
  fail "CLI saw $(clip_count "$1") clip(s) from $1, wanted $2"
}
peer_online() { # NAME PEERNAME
  local i; for i in $(seq 1 300); do
    [[ "$(status_field "$1" online_peers | python3 -I -c 'import json,sys; print(sys.argv[1] in json.load(sys.stdin))' "$2" 2>/dev/null)" == True ]] && return 0
    sleep 0.2
  done
  fail "$1 never saw $2 online"
}
offer_state() { status_field "$1" offers | python3 -I -c 'import json,sys; o=json.load(sys.stdin); print(o[-1]["state"] if o else "none")'; }
history_count() { # NAME PREVIEW: number of history items with exactly this preview
  status_field "$1" history | python3 -I -c 'import json,sys; print(sum(1 for i in json.load(sys.stdin) if i["preview"]==sys.argv[1]))' "$2"
}
wait_wl_text() { # TEXT-PREFIX [SECS]: sets SEEN_MS when L1's clipboard text starts with the prefix
  local i t; for i in $(seq 1 $((${2:-20} * 20))); do
    t="$(wl_a wl-paste --no-newline -t text/plain 2>/dev/null || true)"
    [[ "$t" == "$1"* ]] && { SEEN_MS="$(now_ms)"; SEEN_TEXT="$t"; return 0; }; sleep 0.05
  done
  return 1
}
accept_offer() { # waits for a pending offer, accepts it, waits until done. Sets PEND_MS, DONE_MS.
  local i
  for i in $(seq 1 $((${ACCEPT_WAIT:-60} * 10))); do [[ "$(offer_state L1)" == pending ]] && break; sleep 0.1; done
  [[ "$(offer_state L1)" == pending ]] || return 1
  PEND_MS="$(now_ms)"
  ipc L1 '{"cmd":"offer-accept"}' | grep -q '"ok":true' || fail "offer-accept rejected"
  for i in $(seq 1 600); do [[ "$(offer_state L1)" == done ]] && break; sleep 0.1; done
  [[ "$(offer_state L1)" == done ]] || fail "offer never reached done"
  DONE_MS="$(now_ms)"
}

# ---- account, app sign-in, pairing (devA creates the account, devB is the sender) ------------
EMAIL="linux-xos-$$@example.test"
log "devA creates the account"
login A "$EMAIL"
grep -q '"account":"created"' "$WORK/login-A.json" || fail "devA should create the account"

log "L1: sign in, pair with devA"
app_start L1 "$WORK/data-L1" WAYLAND_DISPLAY="$SA_RT/$SA_WL" SWAYSOCK="$(ls "$SA_RT"/sway-ipc.*.sock | head -1)"
sign_in L1 "$EMAIL"
CODE="$(app_offer L1)"
cli A pair join "$CODE" --yes >"$WORK/pair-A.out" 2>"$WORK/pair-A.err" &
JOIN=$!; PIDS+=("$JOIN")
app_confirm L1
wait "$JOIN" || fail "devA pair join failed: $(cat "$WORK/pair-A.err")"
wait_status L1 state ready 60
wait_status L1 pairing.phase done 20
[[ "$(status_field L1 clip.backend)" == Wayland* ]] || fail "L1 should use the Wayland backend, got '$(status_field L1 clip.backend)'"

log "devB (second CLI device, only paired with L1): sender; devA's engine lock is held by its watch"
login B "$EMAIL" >/dev/null
cli B pair offer --yes >"$WORK/pair-B.out" 2>"$WORK/pair-B.err" &
OFFB=$!; PIDS+=("$OFFB")
wait_for '"event":"offer"' "$WORK/pair-B.out" 30
CODE="$(jfield "$WORK/pair-B.out" offer code)"; [[ -n "$CODE" ]] || fail "devB has no pairing code"
ipc L1 "{\"cmd\":\"pair-join\",\"input\":\"$CODE\"}" | grep -q '"ok":true' || fail "L1 pair-join rejected"
app_confirm L1
wait "$OFFB" || fail "devB pair offer failed: $(cat "$WORK/pair-B.err")"
wait_status L1 pairing.phase done 20

WATCH_OUT="$WORK/watch-A.out"
cli A clip watch --json --timeout 1500 >"$WATCH_OUT" 2>"$WORK/watch-A.err" &
PIDS+=($!)
wait_for '"event":"ready"' "$WATCH_OUT" 40
peer_online L1 devA

# ==== X1: Linux app <-> fsync-cli (this host) ===================================================
log "X1 text app -> CLI"
TXT="x1-out-$$-$RANDOM"; T0="$(now_ms)"
printf %s "$TXT" | wl_a wl-copy
wait_clips devL1 1
[[ "$(last_clip devL1 text)" == "$TXT" ]] || fail "text clip content differs: '$(last_clip devL1 text)'"
[[ "$(last_clip devL1 kind)" == text ]] || fail "text clip kind '$(last_clip devL1 kind)'"
leg x1-text-app-to-cli app-to-cli text "$(( $(last_clip devL1 ts_ms) - T0 ))" true

log "X1 text CLI -> app (marker + privacy hint present, no echo)"
IN="x1-in-$$-$RANDOM"
cli B clip put --text "$IN" --wait 30 >"$WORK/put-B.out" 2>"$WORK/put-B.err" &
PUTB=$!; PIDS+=("$PUTB")
wait_wl_text "$IN" 20 || fail "L1 never received the CLI text"
wait "$PUTB" || fail "devB clip put failed: $(tail -2 "$WORK/put-B.err")"
P0="$(jfield "$WORK/put-B.out" published first_publish_ms)"; [[ "$P0" =~ ^[0-9]+$ ]] || fail "no first_publish_ms from put"
TYPES="$(wl_a wl-paste -l)"
echo "$TYPES" | grep -qx 'application/x-fsync-origin' || fail "marker MIME missing: $TYPES"
echo "$TYPES" | grep -qx 'x-kde-passwordManagerHint' || fail "privacy hint missing: $TYPES"
leg x1-text-cli-to-app cli-to-app text "$((SEEN_MS - P0))" true

log "X1 file CLI -> app (offer-accept over IPC, sha256)"
make_file "$WORK/push.bin" 3000000; PUSH_SHA="$(sha <"$WORK/push.bin")"
S0="$(now_ms)"
cli B send devL1 "$WORK/push.bin" --wait 60 >"$WORK/send-B.out" 2>"$WORK/send-B.err" &
SENDB=$!; PIDS+=("$SENDB")
accept_offer || fail "L1 got no pending offer ($(tail -2 "$WORK/send-B.err"))"
wait "$SENDB" || fail "CLI send failed: $(tail -2 "$WORK/send-B.err")"
GOT="$WORK/home-L1/Downloads/fsync/push.bin"
[[ "$(sha <"$GOT")" == "$PUSH_SHA" ]] || fail "pushed file hash differs on L1"
leg x1-file-cli-to-app cli-to-app file "$((DONE_MS - S0))" true "3 MB; accept step is scripted (offer-accept), pending after $((PEND_MS - S0)) ms"

log "X1 file app -> CLI (--send + pick-device, sha256)"
make_file "$WORK/home-L1/files/sent.bin" 3000000; SENT_SHA="$(sha <"$WORK/home-L1/files/sent.bin")"
mkdir -p "$WORK/fetch-B"
cli B fetch --out "$WORK/fetch-B" --count 1 --timeout 120 >"$WORK/fetch-B.out" 2>"$WORK/fetch-B.err" &
FETCH=$!; PIDS+=("$FETCH")
wait_for '"event":"ready"' "$WORK/fetch-B.out" 40
env HOME="$WORK/home-L1" XDG_RUNTIME_DIR="${APP_RT[L1]}" QT_QPA_PLATFORM=offscreen "$APP_BIN" --data-dir "${APP_DATA[L1]}" --send "$WORK/home-L1/files/sent.bin" \
  || fail "--send was refused"
S0="$(now_ms)"
PICK="$(ipc L1 '{"cmd":"pick-device","input":"devB"}')"; echo "$PICK" | grep -q '"ok":true' || fail "pick-device rejected: $PICK"
wait "$FETCH" || fail "CLI fetch failed: $(tail -2 "$WORK/fetch-B.err")"
S1="$(now_ms)"
[[ "$(sha <"$WORK/fetch-B/sent.bin")" == "$SENT_SHA" ]] || fail "sent file hash differs on the CLI"
leg x1-file-app-to-cli app-to-cli file "$((S1 - S0))" true "3 MB, pick-device to fetch done"

log "X1 no echo / duplicates (settling 4 s)"
sleep 4
assert_eq "$(clip_count devL1)" 1 "clips devA saw from the app (text copy only; received clips and file sends must not be re-sent)"
assert_eq "$(clip_count devB)" 1 "clips devA saw from devB"
assert_eq "$(history_count L1 "$IN")" 1 "history items for the received text"
assert_eq "$(history_count L1 "$TXT")" 1 "history items for the copied text"

# ==== X2 / X5: Linux app <-> fsync-cli on waso-vm ===============================================
if [[ $LOCAL_ONLY -eq 0 ]]; then run_windows_legs; fi

# ---- result file ------------------------------------------------------------------------------
mkdir -p "$(dirname "$RESULT_FILE")"
python3 -I - "$LEGS" "$RESULT_FILE" "$(git -C "$ROOT" rev-parse --short HEAD)" "$LOCAL_ONLY" "$MAX_S" <<'PY'
import json, sys, datetime
legs_f, out, sha, local_only, max_s = sys.argv[1:6]
legs = []
for line in open(legs_f):
    i, d, k, lat, ok, note = (line.rstrip("\n").split("\t") + [""] * 6)[:6]
    legs.append({"id": i, "direction": d, "kind": k, "latency_ms": int(lat), "sha_ok": ok == "true", "note": note})
res = {
    "run": "linux-cross-os", "git_sha": sha, "date": datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
    "scope": "X1" if local_only == "1" else "X1+X2" + ("+X5" if any(l["id"].startswith("x5-") for l in legs) else ""),
    "threshold_ms": int(max_s) * 1000, "checks": {"no_echo_or_duplicates": True, "latency_within_threshold": all(l["latency_ms"] <= int(max_s) * 1000 for l in legs)},
    "legs": legs,
}
json.dump(res, open(out, "w"), indent=2); open(out, "a").write("\n")
PY
log "result: $RESULT_FILE"

Q="$(stop_app L1)"; log "L1 quit: ${Q}s"; assert_lt "$Q" 6.5 "L1 quit duration (s)"
log "PASS"
