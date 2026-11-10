#!/usr/bin/env bash
# App instance helpers for the Linux e2e scripts (sourced after assert.sh / cli-session.sh).
# An instance = the debug app (e2e-control build) in its own dbus session, data dir, and display
# session. Needs: APP_BIN, WORK, API, RELAY, PIDS, and the lib-headless-sessions.sh helpers.
#
#   app_start NAME DATA_DIR [VAR=VALUE...]   (runtime dir $WORK/rt-NAME holds fsync/*.sock)
#   ipc NAME JSON            one request line to the instance's socket
#   status_field NAME PATH   dotted field of the e2e status reply
#   wait_status NAME PATH VALUE [SECS]
#   app_pid NAME

declare -A APP_RT APP_DATA APP_GRP
# Extra arguments for the next app_start (for example a cold-start `--send FILE`).
APP_EXTRA_ARGS=()

# The app gets its OWN runtime dir (the IPC socket lives in it, and D-Bus activated daemons such
# as the document portal mount FUSE filesystems there); the display it may use is passed as
# VAR=VALUE (WAYLAND_DISPLAY as an absolute socket path, or DISPLAY).
app_start() {
  local name="$1" data="$2"; shift 2
  local rt="$WORK/rt-$name"
  mkdir -p "$rt" "$WORK/home-$name" "$WORK/cfg-$name" "$data"; chmod 700 "$rt"
  APP_RT[$name]="$rt"; APP_DATA[$name]="$data"
  (
    export HOME="$WORK/home-$name" XDG_CONFIG_HOME="$WORK/cfg-$name" XDG_RUNTIME_DIR="$rt" \
      FSYNC_SECRETS=file QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic FSYNC_API="$API" FSYNC_RELAY="$RELAY" \
      FSYNC_DEVICE_NAME="dev$name" FSYNC_LOG=info
    # Never inherit the user's real sessions: the caller names the one display this app may see.
    unset DISPLAY WAYLAND_DISPLAY SWAYSOCK HYPRLAND_INSTANCE_SIGNATURE
    for kv in "$@"; do export "${kv?}"; done
    exec setsid dbus-run-session -- "$APP_BIN" --data-dir "$data" ${APP_EXTRA_ARGS[@]+"${APP_EXTRA_ARGS[@]}"}
  ) >>"$WORK/app-$name.log" 2>&1 &
  APP_GRP[$name]=$!
  local i
  for i in $(seq 1 100); do ls "$rt"/fsync/*.sock >/dev/null 2>&1 && break; sleep 0.2; done
  ls "$rt"/fsync/*.sock >/dev/null 2>&1 || fail "app $name did not open its socket"
}

# app_cleanup: kills every app session (bus, daemons it activated) and unmounts their FUSE dirs.
app_cleanup() {
  local name g rt
  for name in "${!APP_GRP[@]}"; do
    g="${APP_GRP[$name]}"; rt="${APP_RT[$name]}"
    kill -TERM -- "-$g" 2>/dev/null || true
    sleep 0.3
    kill -KILL -- "-$g" 2>/dev/null || true
    for m in doc gvfs; do
      fusermount3 -u "$rt/$m" 2>/dev/null || fusermount -u "$rt/$m" 2>/dev/null || true
    done
  done
}

app_pid() { pgrep -f "^$APP_BIN --data-dir ${APP_DATA[$1]}" | head -1 || true; }

ipc() { # NAME JSON
  local sock; sock="$(ls "${APP_RT[$1]}"/fsync/*.sock 2>/dev/null | head -1)"
  [[ -S "$sock" ]] || { echo '{"ok":false,"error":"no socket"}'; return 1; }
  python3 -I - "$sock" "$2" <<'PY'
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

status_field() { # NAME PATH
  ipc "$1" '{"cmd":"status"}' | python3 -I -c '
import json, sys
o = json.load(sys.stdin)["data"]
for k in sys.argv[1].split("."):
    o = o[k]
print(o if not isinstance(o, (dict, list)) else json.dumps(o))' "$2"
}

wait_status() { # NAME PATH VALUE [SECS]
  local i
  for i in $(seq 1 $((${4:-30} * 5))); do
    [[ "$(status_field "$1" "$2" 2>/dev/null || true)" == "$3" ]] && return 0; sleep 0.2
  done
  fail "app $1 status $2 never became '$3' (now '$(status_field "$1" "$2" 2>/dev/null || echo ?)', pairing message: '$(status_field "$1" pairing.message 2>/dev/null || true)')"
}

wait_soft() { # NAME PATH VALUE [SECS]: like wait_status, but returns 1 instead of failing
  local i
  for i in $(seq 1 $((${4:-30} * 5))); do
    [[ "$(status_field "$1" "$2" 2>/dev/null || true)" == "$3" ]] && return 0; sleep 0.2
  done
  return 1
}

# sign_in NAME EMAIL: login over IPC with the dev OTP from the backend log -> needs-pairing/ready.
sign_in() {
  local name="$1" email="$2" code="" i
  wait_status "$name" state signed-out 30
  ipc "$name" "{\"cmd\":\"login\",\"email\":\"$email\"}" | grep -q '"ok":true' || fail "$name login rejected"
  wait_status "$name" state awaiting-code 20
  for i in $(seq 1 20); do code="$(otp_for "$email" | tail -1)"; [[ -n "$code" ]] && break; sleep 0.3; done
  [[ -n "$code" ]] || fail "no OTP for $name"
  ipc "$name" "{\"cmd\":\"login\",\"email\":\"$email\",\"code\":\"$code\"}" | grep -q '"ok":true' || fail "$name verify rejected"
  wait_status "$name" state needs-pairing 40
}

# app_offer NAME: starts a pairing offer, prints the 6 digit code.
app_offer() {
  local name="$1" code="" i
  ipc "$name" '{"cmd":"pair-offer"}' | grep -q '"ok":true' || fail "$name pair-offer rejected"
  # The previous offer's code stays in the status until the new one replaces it.
  wait_status "$name" pairing.phase offering 20
  for i in $(seq 1 100); do code="$(status_field "$name" pairing.code)"; [[ "$code" =~ ^[0-9]{6}$ ]] && break; sleep 0.2; done
  [[ "$code" =~ ^[0-9]{6}$ ]] || fail "$name has no pairing code"
  echo "$code"
}

app_confirm() { # NAME: waits for the SAS prompt, prints the words, confirms
  local name="$1" req
  wait_status "$name" pairing.phase confirm 60
  req="$(status_field "$name" pairing.request_id)"
  ipc "$name" "{\"cmd\":\"pair-confirm\",\"request_id\":\"$req\",\"yes\":true}" | grep -q '"ok":true' || fail "$name pair-confirm rejected"
}

# stop_app NAME: SIGTERM, prints the quit time in seconds, fails when it hangs.
stop_app() {
  local name="$1" pid t0 secs
  pid="$(app_pid "$name")"; [[ "$pid" =~ ^[0-9]+$ ]] || fail "no process for $name"
  t0=$(date +%s.%N); kill -TERM "$pid"
  for _ in $(seq 1 80); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
  ! kill -0 "$pid" 2>/dev/null || fail "$name still running 8 s after SIGTERM"
  secs="$(python3 -c "print(round($(date +%s.%N)-$t0, 2))")"
  echo "$secs"
}

# pair_apps OFFERER JOINER: app to app pairing. A joiner that just signed in may not be online
# at the relay yet ("connection lost"), so the attempt is repeated a few times.
pair_apps() {
  local attempt code
  for attempt in 1 2 3 4 5; do
    code="$(app_offer "$1")"
    ipc "$2" "{\"cmd\":\"pair-join\",\"input\":\"$code\"}" | grep -q '"ok":true' || fail "$2 pair-join rejected"
    if wait_soft "$1" pairing.phase confirm 25; then app_confirm "$1"; app_confirm "$2"; return 0; fi
    log "pairing attempt $attempt failed ($(status_field "$1" pairing.message 2>/dev/null)); retrying"
    ipc "$1" '{"cmd":"cancel"}' >/dev/null || true; ipc "$2" '{"cmd":"cancel"}' >/dev/null || true
    sleep 3
  done
  fail "apps $1 and $2 could not pair"
}
