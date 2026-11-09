#!/usr/bin/env bash
# shellcheck disable=SC2034  # variables are consumed by the sourced libs
# Linux clipboard end-to-end (phase 03): two app instances (debug, e2e-control) and an fsync-cli
# device exchange clips over a local backend + relay. The clipboards are private: a nested
# HEADLESS sway (own XDG_RUNTIME_DIR + WAYLAND_DISPLAY) per instance and an Xvfb for the X11
# subset. The user's real clipboard and displays are never touched.
#
#   apps/linux/tests/e2e/clipboard-with-cli.sh
#
# L1 (sway A) pairs with CLI devA; L2 (sway B) pairs with L1. Wayland: text/html/png out (kind,
# size), hinted copy, bare uri-list and ~/.ssh uri-list not sent, text in, marker + hint present,
# echo not re-sent, html + png L1 -> L2 (sha256), manual pending until --clip-paste, quit while
# owning an entry < 6 s. Phase 04a: history copy-back (text, then files), a CLI file push accepted
# through the IPC `offer-accept` command (sha256 match in the download dir), file clips L1 -> L2
# (file + folder: inbox 0700, uri-list and gnome-copied-files, sha256), a > 20 MiB clip that is
# NOT downloaded until `history-download`. Phase 04b: `--send` + IPC `pick-device` to the CLI
# (`fetch`, sha256) and refused sends (~/.ssh, outside HOME, missing). Then L1 restarts on Xvfb (X11 backend): text out/in,
# hint, marker, quit.
# Needs: docker, dbus-run-session, python3, cargo + qmake6 (QMAKE), curl, sway, wl-clipboard,
# Xvfb, xclip. Missing prerequisite: exit 2 with a hint. Honors CARGO_TARGET_DIR, KEEP=1.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
BUILD="${CARGO_TARGET_DIR:-$ROOT/target}"
mkdir -p "$BUILD" && BUILD="$(cd "$BUILD" && pwd)"
LIN="$ROOT/apps/linux"
APP_BIN="$LIN/target/debug/fsync-linux"
OFFER_BIN="$LIN/target/debug/examples/clip_offer"

. "$HERE/lib-headless-sessions.sh"
hs_prereq
need() { command -v "$1" >/dev/null 2>&1 || { echo "[e2e] missing prerequisite: $1 ($2)" >&2; exit 2; }; }
need docker "needed for Postgres"; need python3 ""; need curl ""; need cargo "rustup"; need sha256sum "coreutils"

WORK="$(mktemp -d)"
PG_NAME="fsync-e2e-linux-clip-pg-$$"
PIDS=()
. "$ROOT/tests/e2e/lib/assert.sh"; . "$ROOT/tests/e2e/lib/cli-session.sh"; . "$(dirname "${BASH_SOURCE[0]}")/lib-postgres-ready.sh"; . "$HERE/lib-app-instance.sh"

cleanup() {
  local rc=$?
  app_cleanup
  for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done
  for p in ${PIDS[@]+"${PIDS[@]}"}; do wait "$p" 2>/dev/null || true; done
  hs_stop_all
  docker rm -f -v "$PG_NAME" >/dev/null 2>&1 || true
  if [[ $rc -ne 0 ]]; then
    log "logs kept in $WORK"
    for f in backend.log relay.log app-L1.log app-L2.log; do [[ -f "$WORK/$f" ]] && { echo "--- $f"; tail -25 "$WORK/$f"; }; done
  elif [[ -z "${KEEP:-}" ]]; then rm -rf "$WORK"; fi
  exit $rc
}
trap cleanup EXIT INT TERM

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
now_ms() { date +%s%3N; }
sha() { sha256sum | cut -d' ' -f1; }

log "building: backend, fsync-cli (release), relay, app (debug, e2e-control), clip_offer"
(cd "$ROOT" && cargo build --release -p fsync-backend -p fsync-cli)
(cd "$ROOT/spikes/iroh-discovery" && CARGO_TARGET_DIR="$BUILD/spike" cargo build --release)
(cd "$LIN" && QMAKE="${QMAKE:-/usr/bin/qmake6}" cargo build --features e2e-control --example clip_offer -p fsync-linux-platform \
  && QMAKE="${QMAKE:-/usr/bin/qmake6}" cargo build --features e2e-control)
CLI="$BUILD/release/fsync-cli"; BACKEND="$BUILD/release/fsync-backend"
SPIKE="$BUILD/spike/release/iroh-discovery-spike"
for b in "$CLI" "$BACKEND" "$SPIKE" "$APP_BIN" "$OFFER_BIN"; do [[ -x "$b" ]] || fail "missing binary $b"; done
"$APP_BIN" --build-info | grep -q e2e-control || fail "app binary was not built with e2e-control"

log "postgres, backend (dev mode), relay"
docker run -d --name "$PG_NAME" -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=fsync -p 127.0.0.1::5432 postgres:17 >/dev/null
PGPORT="$(docker port "$PG_NAME" 5432/tcp | head -1 | sed 's/.*://')"
wait_pg_ready "$PG_NAME"
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

log "headless sessions: sway A (L1), sway B (L2), Xvfb (X11 subset)"
hs_start_sway A; hs_start_sway B; hs_start_xvfb X
SA_RT="$HS_A_RT"; SA_WL="$HS_A_WL"; SB_RT="$HS_B_RT"; SB_WL="$HS_B_WL"; XDISP="$HS_X_DISPLAY"
wl_a() { env -u DISPLAY XDG_RUNTIME_DIR="$SA_RT" WAYLAND_DISPLAY="$SA_WL" "$@"; }
wl_b() { env -u DISPLAY XDG_RUNTIME_DIR="$SB_RT" WAYLAND_DISPLAY="$SB_WL" "$@"; }
x11() { env -u WAYLAND_DISPLAY DISPLAY="$XDISP" "$@"; }

# Sources that own a multi-MIME selection (wl-copy/xclip offer one type): kept until killed.
OFFERS=()
offer_wl() { # SESSION(a|b) PART... : serves until stop_offers
  local s="$1"; shift
  local rt wl; if [[ "$s" == a ]]; then rt="$SA_RT"; wl="$SA_WL"; else rt="$SB_RT"; wl="$SB_WL"; fi
  "$OFFER_BIN" --backend wayland --socket "$rt/$wl" "$@" >"$WORK/offer.out" 2>&1 &
  OFFERS+=($!); PIDS+=($!)
  wait_for '^ready' "$WORK/offer.out" 10
}
offer_x11() { "$OFFER_BIN" --backend x11 --display "$XDISP" "$@" >"$WORK/offer.out" 2>&1 & OFFERS+=($!); PIDS+=($!); wait_for '^ready' "$WORK/offer.out" 10; }
stop_offers() { for p in ${OFFERS[@]+"${OFFERS[@]}"}; do kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; done; OFFERS=(); : >"$WORK/offer.out"; }

# Only clips sent by L1 count: devB (same account) also reaches devA with the clips it puts.
clip_count() { grep '"event":"clip"' "$WATCH_OUT" | grep -c '"from_name":"devL1"' || true; }
wait_clips() { # COUNT [SECS]: the CLI watch has seen at least COUNT clips
  local i; for i in $(seq 1 $((${2:-15} * 5))); do [[ "$(clip_count)" -ge "$1" ]] && return 0; sleep 0.2; done
  fail "CLI saw $(clip_count) clip(s), wanted $1"
}
assert_no_new_clip() { # COUNT_BEFORE WHAT: nothing arrived for 3 s
  sleep 3; [[ "$(clip_count)" -eq "$1" ]] || fail "$2 was sent but must not be (CLI saw $(clip_count), had $1)"
}
last_clip() { python3 -I - "$WATCH_OUT" "$1" <<'PY'
import json, sys
last = None
for line in open(sys.argv[1]):
    try: o = json.loads(line)
    except ValueError: continue
    if o.get("event") == "clip" and o.get("from_name") == "devL1": last = o
print("" if last is None else last.get(sys.argv[2], ""))
PY
}
peer_online() { # NAME PEERNAME: the backend reports the peer online (publish connects on demand)
  local i; for i in $(seq 1 300); do
    [[ "$(status_field "$1" online_peers | python3 -I -c 'import json,sys; print(sys.argv[1] in json.load(sys.stdin))' "$2" 2>/dev/null)" == True ]] && return 0
    sleep 0.2
  done
  fail "$1 never saw $2 online"
}
make_png() { python3 -I - "$1" "$2" <<'PY'
import struct, sys, zlib
w = h = int(sys.argv[2])
raw = b"".join(b"\x00" + bytes((x * 7 + y * 3) % 256 for x in range(w) for _ in range(3))[: w * 3] for y in range(h))
def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
open(sys.argv[1], "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
PY
}


# --- phase 04a helpers: offers, history, inbox ---
offer_state() { # NAME: state of the newest offer ("none" without one)
  status_field "$1" offers | python3 -I -c 'import json,sys; o=json.load(sys.stdin); print(o[-1]["state"] if o else "none")'
}
history_id() { # NAME PREVIEW|kind=Files: id of the newest matching history item
  status_field "$1" history | python3 -I -c '
import json, sys
want = sys.argv[1]
for i in json.load(sys.stdin):
    if (want == "kind=Files" and i["kind"] == "Files") or i["preview"] == want:
        print(i["id"]); break' "$2"
}
# uri_paths SESSION(a|b): the local paths of the current uri-list, one per line
uri_paths() {
  local f=wl_$1
  $f wl-paste -t text/uri-list 2>/dev/null | python3 -I -c '
import sys, urllib.parse
for l in sys.stdin.read().split("\r\n"):
    if l.startswith("file://"): print(urllib.parse.unquote(l[7:]))'
}
wait_uri_paths() { # SESSION PATTERN [SECS]: the uri-list matches the regex
  local i; for i in $(seq 1 $((${3:-15} * 5))); do uri_paths "$1" | grep -q "$2" && return 0; sleep 0.2; done
  fail "session $1 never offered a uri-list matching '$2' (has: $(uri_paths "$1" | tr '\n' ' '))"
}
make_file() { # PATH BYTES
  mkdir -p "$(dirname "$1")"; head -c "$2" /dev/urandom >"$1"
}

EMAIL="linux-clip-$$@example.test"
log "devA creates the account"
login A "$EMAIL"
grep -q '"account":"created"' "$WORK/login-A.json" || fail "devA should create the account"

log "L1 (sway A): sign in, pair with devA"
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
log "L1 ready; backend: $(status_field L1 clip.backend); filter: $(status_field L1 clip.capability)"

log "devB (a second CLI device, only paired with L1) will be the sender: devA's engine lock is held by its watch"
login B "$EMAIL" >/dev/null
# A device that needs pairing offers; the paired app joins (and confirms the words).
cli B pair offer --yes >"$WORK/pair-B.out" 2>"$WORK/pair-B.err" &
OFFB=$!; PIDS+=("$OFFB")
wait_for '"event":"offer"' "$WORK/pair-B.out" 30
CODE="$(jfield "$WORK/pair-B.out" offer code)"
[[ -n "$CODE" ]] || fail "devB has no pairing code"
ipc L1 "{\"cmd\":\"pair-join\",\"input\":\"$CODE\"}" | grep -q '"ok":true' || fail "L1 pair-join rejected"
app_confirm L1
wait "$OFFB" || fail "devB pair offer failed: $(cat "$WORK/pair-B.err")"
wait_status L1 pairing.phase done 20

log "L2 (sway B): sign in, pair with L1"
app_start L2 "$WORK/data-L2" WAYLAND_DISPLAY="$SB_RT/$SB_WL" SWAYSOCK="$(ls "$SB_RT"/sway-ipc.*.sock | head -1)"
sign_in L2 "$EMAIL"
pair_apps L2 L1   # L2 offers, L1 joins (an app that just signed in joins unreliably, see lib)
wait_status L2 state ready 60

WATCH_OUT="$WORK/watch-A.out"
cli A clip watch --json --timeout 1500 >"$WATCH_OUT" 2>"$WORK/watch-A.err" &
PIDS+=($!)
wait_for '"event":"ready"' "$WATCH_OUT" 40
peer_online L1 devA; peer_online L1 devL2; peer_online L2 devL1

log "WAYLAND: text out"
T0="$(now_ms)"; TXT="hello-$$-$RANDOM"
printf %s "$TXT" | wl_a wl-copy
wait_clips 1
assert_eq "$(last_clip text)" "$TXT" "text clip content"
assert_eq "$(last_clip kind)" text "text clip kind"
log "text latency copy -> CLI: $(( $(last_clip ts_ms) - T0 )) ms"
LAT=$(( $(last_clip ts_ms) - T0 )); [[ $LAT -lt 1000 ]] || fail "text latency ${LAT} ms is over 1 s"

log "WAYLAND: html out (kind rich), png out (kind image)"
printf '<b>bold</b> and text' >"$WORK/p.html"; printf 'bold and text' >"$WORK/p.txt"
offer_wl a text/html="$WORK/p.html" text/plain="$WORK/p.txt"
wait_clips 2; assert_eq "$(last_clip kind)" rich "html clip kind"; stop_offers
make_png "$WORK/p.png" 40
offer_wl a image/png="$WORK/p.png"
wait_clips 3; assert_eq "$(last_clip kind)" image "png clip kind"
assert_eq "$(last_clip total_size)" "$(stat -c %s "$WORK/p.png")" "png clip size"; stop_offers

log "WAYLAND: hinted copy, bare uri-list, ~/.ssh uri-list are not sent"
N="$(clip_count)"
printf 'topsecret' >"$WORK/secret.txt"
offer_wl a text/plain="$WORK/secret.txt" --hint; assert_no_new_clip "$N" "a hinted copy"; stop_offers
mkdir -p "$WORK/home-L1/.ssh"; printf key >"$WORK/home-L1/.ssh/id_x"; printf ok >"$WORK/home-L1/doc.txt"
printf 'file://%s/doc.txt\r\n' "$WORK/home-L1" >"$WORK/bare.uris"
offer_wl a text/uri-list="$WORK/bare.uris"; assert_no_new_clip "$N" "a uri-list without a file-manager type"; stop_offers
printf 'file://%s/.ssh/id_x\r\n' "$WORK/home-L1" >"$WORK/ssh.uris"; printf 'copy' >"$WORK/copy.txt"
offer_wl a text/uri-list="$WORK/ssh.uris" x-special/gnome-copied-files="$WORK/copy.txt"
assert_no_new_clip "$N" "a ~/.ssh file"; stop_offers

log "WAYLAND: text in, marker + hint present, no echo"
N="$(clip_count)"; IN="from-cli-$$-$RANDOM"
cli B clip put --text "$IN" >/dev/null
for _ in $(seq 1 50); do [[ "$(wl_a wl-paste --no-newline -t text/plain 2>/dev/null || true)" == "$IN" ]] && break; sleep 0.2; done
assert_eq "$(wl_a wl-paste --no-newline -t text/plain)" "$IN" "text received by L1"
TYPES="$(wl_a wl-paste -l)"
echo "$TYPES" | grep -qx 'application/x-fsync-origin' || fail "marker MIME missing: $TYPES"
echo "$TYPES" | grep -qx 'x-kde-passwordManagerHint' || fail "privacy hint missing: $TYPES"
assert_no_new_clip "$N" "the received clip (echo)"

log "HISTORY: copy-back puts an older item back on the clipboard (no re-send, marker present)"
OTHER="other-$$-$RANDOM"
printf %s "$OTHER" | wl_a wl-copy
for _ in $(seq 1 50); do [[ "$(wl_a wl-paste --no-newline -t text/plain 2>/dev/null || true)" == "$OTHER" ]] && break; sleep 0.2; done
HID="$(history_id L1 "$IN")"; [[ -n "$HID" ]] || fail "L1 history has no item with the CLI text"
sleep 2; N="$(clip_count)"
ipc L1 "{\"cmd\":\"history-copy\",\"input\":\"$HID\"}" | grep -q '"ok":true' || fail "history-copy rejected"
for _ in $(seq 1 50); do [[ "$(wl_a wl-paste --no-newline -t text/plain 2>/dev/null || true)" == "$IN" ]] && break; sleep 0.2; done
assert_eq "$(wl_a wl-paste --no-newline -t text/plain)" "$IN" "history copy-back"
wl_a wl-paste -l | grep -qx 'application/x-fsync-origin' || fail "copy-back lost the marker"
assert_no_new_clip "$N" "the copy-back (echo)"

log "PUSH: CLI send -> L1 offer -> offer-accept (IPC) -> file in the download dir (sha256)"
make_file "$WORK/push.bin" 3000000; PUSH_SHA="$(sha <"$WORK/push.bin")"
cli B send devL1 "$WORK/push.bin" --wait 60 >"$WORK/send-B.out" 2>"$WORK/send-B.err" &
SENDB=$!; PIDS+=("$SENDB")
for _ in $(seq 1 150); do [[ "$(offer_state L1)" == pending ]] && break; sleep 0.2; done
[[ "$(offer_state L1)" == pending ]] || fail "L1 got no pending offer ($(cat "$WORK/send-B.err" | tail -2))"
ipc L1 '{"cmd":"offer-accept"}' | grep -q '"ok":true' || fail "offer-accept rejected"
for _ in $(seq 1 150); do [[ "$(offer_state L1)" == done ]] && break; sleep 0.2; done
assert_eq "$(offer_state L1)" done "offer state after accept"
wait "$SENDB" || fail "CLI send failed: $(tail -2 "$WORK/send-B.err")"
assert_eq "$(sha <"$WORK/home-L1/Downloads/fsync/push.bin")" "$PUSH_SHA" "pushed file hash"
assert_eq "$(stat -c %a "$WORK/home-L1/Downloads/fsync")" 700 "download dir mode"

log "WAYLAND: html + png L1 -> L2 (sha256), then manual pending"
HTML_SHA="$(sha <"$WORK/p.html")"; PNG_SHA="$(sha <"$WORK/p.png")"
offer_wl a text/html="$WORK/p.html" text/plain="$WORK/p.txt"
for _ in $(seq 1 75); do [[ "$(wl_b wl-paste --no-newline -t text/html 2>/dev/null | sha)" == "$HTML_SHA" ]] && break; sleep 0.2; done
assert_eq "$(wl_b wl-paste --no-newline -t text/html | sha)" "$HTML_SHA" "html bytes on L2"; stop_offers
offer_wl a image/png="$WORK/p.png"
for _ in $(seq 1 75); do [[ "$(wl_b wl-paste -t image/png 2>/dev/null | sha)" == "$PNG_SHA" ]] && break; sleep 0.2; done
assert_eq "$(wl_b wl-paste -t image/png | sha)" "$PNG_SHA" "png bytes on L2"; stop_offers

log "FILES L1 -> L2: a file and a folder arrive in L2's inbox (0700), offered as uri-list + gnome-copied-files"
make_file "$WORK/home-L1/files/clip-a.txt" 5000; A_SHA="$(sha <"$WORK/home-L1/files/clip-a.txt")"
printf 'file://%s/files/clip-a.txt\r\n' "$WORK/home-L1" >"$WORK/a.uris"
printf 'copy\nfile://%s/files/clip-a.txt' "$WORK/home-L1" >"$WORK/a.gnome"
offer_wl a text/uri-list="$WORK/a.uris" x-special/gnome-copied-files="$WORK/a.gnome"
wait_uri_paths b "/data-L2/inbox/.*/clip-a.txt"
A_PATH="$(uri_paths b | head -1)"
assert_eq "$(sha <"$A_PATH")" "$A_SHA" "file clip content on L2"
wl_b wl-paste -l | grep -qx 'x-special/gnome-copied-files' || fail "L2 does not offer gnome-copied-files"
wl_b wl-paste -t x-special/gnome-copied-files | grep -q '^copy' || fail "gnome-copied-files body has no action line"
wl_b wl-paste -l | grep -qx 'application/x-fsync-origin' || fail "L2 file entry has no marker"
assert_eq "$(stat -c %a "$WORK/data-L2/inbox")" 700 "inbox mode"
assert_eq "$(stat -c %a "$(dirname "$A_PATH")")" 700 "inbox item dir mode"
status_field L2 clip.last_event | grep -q "Received files" || fail "L2 status: '$(status_field L2 clip.last_event)'"
stop_offers
FILE_ID="$(history_id L2 kind=Files)"; [[ -n "$FILE_ID" ]] || fail "L2 history has no files item"

mkdir -p "$WORK/home-L1/files/folder/sub"; make_file "$WORK/home-L1/files/folder/f1.bin" 2000; make_file "$WORK/home-L1/files/folder/sub/f2.bin" 3000
F2_SHA="$(sha <"$WORK/home-L1/files/folder/sub/f2.bin")"
printf 'file://%s/files/folder\r\n' "$WORK/home-L1" >"$WORK/d.uris"
printf 'copy\nfile://%s/files/folder' "$WORK/home-L1" >"$WORK/d.gnome"
offer_wl a text/uri-list="$WORK/d.uris" x-special/gnome-copied-files="$WORK/d.gnome"
wait_uri_paths b "/data-L2/inbox/.*/folder$"
D_PATH="$(uri_paths b | head -1)"; assert_eq "$(uri_paths b | wc -l)" 1 "a folder is one uri-list entry"
assert_eq "$(sha <"$D_PATH/sub/f2.bin")" "$F2_SHA" "folder content on L2"
stop_offers

log "HISTORY: copy-back of the first file clip on L2 (inbox copy reused, no new download)"
printf %s "other-$$" | wl_b wl-copy
# L1 receives that text and writes its own clipboard: let it finish before L1 is used as a source
# again (a late write would replace the large clip offered below).
for _ in $(seq 1 75); do [[ "$(wl_a wl-paste --no-newline -t text/plain 2>/dev/null || true)" == "other-$$" ]] && break; sleep 0.2; done
sleep 1
# L2's own publish of that text must settle too: its outcome is written to L2's status when the
# peers ack, which can take seconds. Still pending in the LARGE step, it overwrites the "Large file
# clip" status (one status slot, last writer wins) and the check below misses the large clip.
for _ in $(seq 1 150); do [[ "$(status_field L2 clip.last_event)" == "Sent to 2 devices." ]] && break; sleep 0.2; done
assert_eq "$(status_field L2 clip.last_event)" "Sent to 2 devices." "L2's publish of the text settled"
ipc L2 "{\"cmd\":\"history-copy\",\"input\":\"$FILE_ID\"}" | grep -q '"ok":true' || fail "history-copy of the files item rejected"
wait_uri_paths b "/inbox/$FILE_ID/clip-a.txt"

log "LARGE: a > 20 MiB file clip is not downloaded; history-download fetches it into the inbox"
INBOX_N="$(ls "$WORK/data-L2/inbox" | wc -l)"
head -c 22020096 /dev/zero >"$WORK/home-L1/files/big.bin"; BIG_SHA="$(sha <"$WORK/home-L1/files/big.bin")"
printf 'file://%s/files/big.bin\r\n' "$WORK/home-L1" >"$WORK/big.uris"
printf 'copy\nfile://%s/files/big.bin' "$WORK/home-L1" >"$WORK/big.gnome"
offer_wl a text/uri-list="$WORK/big.uris" x-special/gnome-copied-files="$WORK/big.gnome"
for _ in $(seq 1 150); do status_field L2 clip.last_event | grep -q "Large file clip" && break; sleep 0.2; done
status_field L2 clip.last_event | grep -q "Large file clip" || fail "L2 should report the large clip (got '$(status_field L2 clip.last_event)'; L1 says '$(status_field L1 clip.last_event)')"
sleep 3
assert_eq "$(ls "$WORK/data-L2/inbox" | wc -l)" "$INBOX_N" "inbox entries (the large clip must not be downloaded)"
uri_paths b | grep -q "big.bin" && fail "L2 clipboard was replaced by the large clip"
BIG_ID="$(history_id L2 kind=Files)"; [[ -n "$BIG_ID" && "$BIG_ID" != "$FILE_ID" ]] || fail "L2 history has no new files item"
DL="$(ipc L2 "{\"cmd\":\"history-download\",\"input\":\"$BIG_ID\"}")"
echo "$DL" | grep -q '"ok":true' || fail "history-download rejected: $DL"
for _ in $(seq 1 300); do [[ -f "$WORK/data-L2/inbox/$BIG_ID/.complete" ]] && break; sleep 0.5; done
[[ -f "$WORK/data-L2/inbox/$BIG_ID/.complete" ]] || fail "the large clip was not downloaded within 150 s"
assert_eq "$(sha <"$WORK/data-L2/inbox/$BIG_ID/big.bin")" "$BIG_SHA" "downloaded large file"
stop_offers

ipc L2 '{"cmd":"clip-mode","input":"manual"}' | grep -q '"ok":true' || fail "clip-mode rejected"
MAN="manual-$$-$RANDOM"
printf %s "$MAN" | wl_a wl-copy
for _ in $(seq 1 75); do [[ -n "$(status_field L2 clip.pending)" ]] && break; sleep 0.2; done
[[ -n "$(status_field L2 clip.pending)" ]] || fail "L2 should hold the clip as pending in manual mode"
[[ "$(wl_b wl-paste --no-newline -t text/plain 2>/dev/null || true)" != "$MAN" ]] || fail "manual mode wrote the clipboard on its own"
env HOME="$WORK/home-L2" XDG_RUNTIME_DIR="${APP_RT[L2]}" QT_QPA_PLATFORM=offscreen "$APP_BIN" --data-dir "$WORK/data-L2" --clip-paste || fail "--clip-paste failed"
assert_eq "$(wl_b wl-paste --no-newline -t text/plain)" "$MAN" "pasted pending clip"
[[ -z "$(status_field L2 clip.pending)" ]] || fail "pending should be empty after paste"

log "SEND (phase 04b): --send stages the files and opens the picker; pick-device sends them to the CLI (sha256)"
app_cmd() { # NAME ARGS...: a second launch of the app for instance NAME (forwarded over IPC)
  local n="$1"; shift
  env HOME="$WORK/home-$n" XDG_RUNTIME_DIR="${APP_RT[$n]}" QT_QPA_PLATFORM=offscreen "$APP_BIN" --data-dir "${APP_DATA[$n]}" "$@"
}
make_file "$WORK/home-L1/files/sent.bin" 300000; SENT_SHA="$(sha <"$WORK/home-L1/files/sent.bin")"
mkdir -p "$WORK/fetch-B"
cli B fetch --out "$WORK/fetch-B" --count 1 --timeout 120 >"$WORK/fetch-B.out" 2>"$WORK/fetch-B.err" &
FETCH=$!; PIDS+=("$FETCH")
wait_for '"event":"ready"' "$WORK/fetch-B.out" 40
app_cmd L1 --send "$WORK/home-L1/files/sent.bin" || fail "--send of a home file was refused"
PICK="$(ipc L1 '{"cmd":"pick-device","input":"devB"}')"
echo "$PICK" | grep -q '"ok":true' || fail "pick-device rejected: $PICK"
wait "$FETCH" || fail "CLI fetch failed: $(tail -2 "$WORK/fetch-B.err")"
assert_eq "$(sha <"$WORK/fetch-B/sent.bin")" "$SENT_SHA" "file sent with --send"
for _ in $(seq 1 100); do status_field L1 clip.last_event | grep -q "Files sent" && break; sleep 0.2; done
status_field L1 clip.last_event | grep -q "Files sent" || fail "L1 status after the send: '$(status_field L1 clip.last_event)' (wanted 'Files sent.')"

log "SEND: ~/.ssh file, a file outside HOME and a missing path are refused; nothing stays staged"
OUT="$(app_cmd L1 --send "$WORK/home-L1/.ssh/id_x" 2>&1)" && fail "--send of a ~/.ssh file must exit non-zero"
echo "$OUT" | grep -q "path-protected" || fail "~/.ssh refusal text: '$OUT'"
printf outside >"$WORK/outside.txt"
OUT="$(app_cmd L1 --send "$WORK/outside.txt" 2>&1)" && fail "--send outside HOME must exit non-zero"
echo "$OUT" | grep -q "path-protected" || fail "outside-HOME refusal text: '$OUT'"
app_cmd L1 --send "$WORK/home-L1/nonexistent" >/dev/null 2>&1 && fail "--send of a missing path must exit non-zero"
PICK="$(ipc L1 '{"cmd":"pick-device","input":"devB"}')"
echo "$PICK" | grep -q '"error":"nothing-staged"' || fail "a refused send must stage nothing: $PICK"

log "WAYLAND: quit while owning an entry stays within the 6 s deadline"
Q="$(stop_app L2)"; log "L2 quit: ${Q}s"; assert_lt "$Q" 6.5 "L2 quit duration (s)"
stop_offers

log "MANUAL BACKEND (the GNOME path): L2 restarts with FSYNC_CLIPBOARD_BACKEND=manual; paste and send go through the QuickClipWindow (offscreen Qt gives it the focus)"
app_start L2 "$WORK/data-L2" FSYNC_CLIPBOARD_BACKEND=manual
wait_status L2 state ready 60
[[ "$(status_field L2 clip.backend)" == Manual* ]] || fail "L2 should be on the Manual backend, got '$(status_field L2 clip.backend)'"
peer_online L1 devL2
GN="gnome-$$-$RANDOM"
printf %s "$GN" | wl_a wl-copy
for _ in $(seq 1 75); do [[ -n "$(status_field L2 clip.pending)" ]] && break; sleep 0.2; done
[[ -n "$(status_field L2 clip.pending)" ]] || fail "the manual backend should hold the received clip as pending"
env HOME="$WORK/home-L2" XDG_RUNTIME_DIR="${APP_RT[L2]}" QT_QPA_PLATFORM=offscreen "$APP_BIN" --data-dir "$WORK/data-L2" --clip-paste || fail "--clip-paste (window path) failed"
wait_status L2 clip.pending "" 15
status_field L2 clip.last_event | grep -q "Pasted" || fail "the window should report the paste (got '$(status_field L2 clip.last_event)')"
# What the window wrote carries our marker: sending it must be refused as our own echo.
env HOME="$WORK/home-L2" XDG_RUNTIME_DIR="${APP_RT[L2]}" QT_QPA_PLATFORM=offscreen "$APP_BIN" --data-dir "$WORK/data-L2" --clip-send || fail "--clip-send (window path) failed"
for _ in $(seq 1 50); do status_field L2 clip.last_event | grep -q "just received" && break; sleep 0.2; done
status_field L2 clip.last_event | grep -q "just received" || fail "the window send should read the clipboard back and refuse our own entry (got '$(status_field L2 clip.last_event)')"
Q="$(stop_app L2)"; log "L2 (manual backend) quit: ${Q}s"; assert_lt "$Q" 6.5 "L2 quit duration (s)"

Q="$(stop_app L1)"; log "L1 quit: ${Q}s"; assert_lt "$Q" 6.5 "L1 quit duration (s)"

log "X11 (Xvfb, FSYNC_CLIPBOARD_BACKEND=x11): L1 restarts on the same data dir; the launch carries --send (cold start: staged once ready)"
make_file "$WORK/home-L1/files/cold.bin" 120000; COLD_SHA="$(sha <"$WORK/home-L1/files/cold.bin")"
APP_EXTRA_ARGS=(--send "$WORK/home-L1/files/cold.bin")
app_start L1 "$WORK/data-L1" DISPLAY="$XDISP" FSYNC_CLIPBOARD_BACKEND=x11
APP_EXTRA_ARGS=()
wait_status L1 state ready 60
[[ "$(status_field L1 clip.backend)" == X11* ]] || fail "L1 should use the X11 backend, got '$(status_field L1 clip.backend)' ($(status_field L1 clip.reason); Xvfb: $(tail -3 "${HS_DIRS[2]}/xvfb.log" 2>&1 | tr '\n' ' '); alive: $(pgrep -c Xvfb); display $XDISP; direct xclip: $(x11 xclip -selection clipboard -o 2>&1 | head -2 | tr '\n' ' '); sockets: $(ls /tmp/.X11-unix 2>&1 | tr '\n' ' '))"
log "L1 backend: $(status_field L1 clip.backend); filter: $(status_field L1 clip.capability)"
peer_online L1 devA; sleep 2

log "SEND (cold start): the --send of the launch is staged after ready; pick-device sends it to the CLI (sha256)"
mkdir -p "$WORK/fetch-B2"
cli B fetch --out "$WORK/fetch-B2" --count 1 --timeout 120 >"$WORK/fetch-B2.out" 2>"$WORK/fetch-B2.err" &
FETCH=$!; PIDS+=("$FETCH")
wait_for '"event":"ready"' "$WORK/fetch-B2.out" 40
PICK=""; for _ in $(seq 1 100); do
  PICK="$(ipc L1 '{"cmd":"pick-device","input":"devB"}')"; echo "$PICK" | grep -q '"ok":true' && break; sleep 0.3
done
echo "$PICK" | grep -q '"ok":true' || fail "cold-start --send was never staged: $PICK"
wait "$FETCH" || fail "CLI fetch (cold start) failed: $(tail -2 "$WORK/fetch-B2.err")"
assert_eq "$(sha <"$WORK/fetch-B2/cold.bin")" "$COLD_SHA" "file sent by the cold-start --send"

log "X11: text out"
N="$(clip_count)"; T0="$(now_ms)"; TXT="x11-hello-$$-$RANDOM"
printf %s "$TXT" | x11 xclip -selection clipboard -i
wait_clips $((N + 1))
assert_eq "$(last_clip text)" "$TXT" "x11 text clip content"
LAT=$(( $(last_clip ts_ms) - T0 )); log "x11 text latency copy -> CLI: ${LAT} ms"
[[ $LAT -lt 1000 ]] || fail "x11 text latency ${LAT} ms is over 1 s"

log "X11: hinted copy is not sent"
N="$(clip_count)"
offer_x11 text/plain="$WORK/secret.txt" --hint; assert_no_new_clip "$N" "a hinted X11 copy"; stop_offers

log "X11: text in, marker present, no echo"
N="$(clip_count)"; IN="x11-from-cli-$$-$RANDOM"
cli B clip put --text "$IN" >/dev/null
for _ in $(seq 1 50); do [[ "$(x11 xclip -selection clipboard -o -t UTF8_STRING 2>/dev/null || true)" == "$IN" ]] && break; sleep 0.2; done
assert_eq "$(x11 xclip -selection clipboard -o -t UTF8_STRING)" "$IN" "x11 text received by L1"
x11 xclip -selection clipboard -o -t TARGETS | grep -qx 'application/x-fsync-origin' || fail "x11 marker target missing"
assert_no_new_clip "$N" "the received clip (x11 echo)"

log "X11: quit while owning an entry stays within the 6 s deadline"
Q="$(stop_app L1)"; log "L1 quit: ${Q}s"; assert_lt "$Q" 6.5 "L1 (x11) quit duration (s)"

log "PASS"
