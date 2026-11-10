#!/usr/bin/env bash
# Sourced by cross-os-with-windows.sh (after lib-cross-os-windows-legs.sh). Defines run_windows_gui_legs
# (X3 + X4, `X3=1`): the Linux app <-> the WinUI app on the Rig worker. The worker session is
# INTERACTIVE (taskMode interactive, logged-on dev user; the coordinator switches it, never this
# script) and its clipboard is shared with the dev user: nothing else may run during these legs.
#
# Ordering without cross-machine barriers inside a `remote run`: sync files in
# %TEMP%\fsync-cross-os\gui\sync on the worker, read and written over plain ssh (see
# apps/windows/tests/e2e/cross-os-gui-peer.ps1). Latencies use timestamps taken on the worker and are
# corrected with the measured clock skew (win_measure_skew), exactly like the CLI legs.
# Needs from the caller: win_remote, win_health_gate, win_measure_skew, win_field, WIN_SSH, WIN_SKEW_MS,
# WIN_SKEW_ERR_MS, leg, wl_a, SA_RT, SA_WL, WORK, API, RELAY, EMAIL, OFFER_BIN, app_confirm, wait_status,
# status_field, ipc, otp_for, now_ms, sha, make_file, PIDS.

WIN_GUI="apps/windows/tests/e2e/cross-os-gui-peer.ps1"
WIN_GUI_NAME="devWG"
GUI_OFFERS=()

# --- ssh access to the sync dir (names are [a-z0-9-] only) ---
gui_sync_expr() { echo "(Join-Path \$env:TEMP 'fsync-cross-os\\gui\\sync\\$1')"; }
gui_sync_get() { # NAME: first line, empty when absent
  [[ "$1" =~ ^[a-z0-9-]+$ ]] || fail "bad sync name $1"
  ssh -o BatchMode=yes -o ConnectTimeout=10 "$WIN_SSH" "powershell -NoProfile -Command \"Get-Content -ErrorAction SilentlyContinue $(gui_sync_expr "$1")\"" 2>/dev/null \
    | tr -d '\r' | head -1 || true
}
gui_sync_put() { # NAME [VALUE]
  [[ "$1" =~ ^[a-z0-9-]+$ && "${2:-1}" =~ ^[A-Za-z0-9\ ._-]+$ ]] || fail "bad sync put $1"
  ssh -o BatchMode=yes -o ConnectTimeout=10 "$WIN_SSH" "powershell -NoProfile -Command \"Set-Content -Encoding ASCII -Path $(gui_sync_expr "$1") -Value '${2:-1}'\"" >/dev/null 2>&1 \
    || fail "ssh sync put $1 failed"
}
gui_sync_reset() {
  ssh -o BatchMode=yes -o ConnectTimeout=10 "$WIN_SSH" "powershell -NoProfile -Command \"Remove-Item -Recurse -Force -ErrorAction SilentlyContinue (Join-Path \$env:TEMP 'fsync-cross-os\\gui\\sync')\"" >/dev/null 2>&1 || true
}
gui_sync_wait() { # NAME [SECS]: prints the value; fails when the windows run ends first
  local v i
  for i in $(seq 1 "${2:-120}"); do
    v="$(gui_sync_get "$1")"; [[ -n "$v" ]] && { echo "$v"; return 0; }
    kill -0 "$GUI_PID" 2>/dev/null || { wait "$GUI_PID" || true; fail "windows gui run ended while waiting for '$1': $(tail -c 600 "$GUI_OUT" | tr '\n' ' ')"; }
    sleep 1
  done
  fail "timeout waiting for sync file '$1'"
}

# gui_run_start TAG ARGS...: one remote run in the background; GUI_PID/GUI_OUT name it.
gui_run_start() {
  GUI_OUT="$WORK/win-gui-$1.out"; shift
  gui_sync_reset
  win_remote 600 "$GUI_OUT" -- "$WIN_GUI" "$@" &
  GUI_PID=$!; PIDS+=("$GUI_PID")
}
gui_run_end() { wait "$GUI_PID" || fail "windows gui run failed: $(tail -c 800 "$GUI_OUT" | tr '\n' ' ')"; }

# --- test items: png pixel (x*7+y*3)%256 on every channel; verification of a decoded png ---
gui_make_png() { python3 -I - "$1" "$2" <<'PY'
import struct, sys, zlib
w = h = int(sys.argv[2])
raw = b"".join(b"\x00" + bytes((x * 7 + y * 3) % 256 for x in range(w) for _ in range(3)) for y in range(h))
def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
open(sys.argv[1], "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
PY
}
gui_png_ok() { python3 -I - "$1" "$2" <<'PY'
import struct, sys, zlib
d = open(sys.argv[1], "rb").read(); n = int(sys.argv[2])
if d[:8] != b"\x89PNG\r\n\x1a\n": sys.exit(1)
pos, idat, hdr = 8, b"", None
while pos < len(d):
    ln, t = struct.unpack(">I4s", d[pos:pos + 8]); body = d[pos + 8:pos + 8 + ln]; pos += 12 + ln
    if t == b"IHDR": hdr = struct.unpack(">IIBBBBB", body)
    elif t == b"IDAT": idat += body
w, h, depth, ctype, _, _, inter = hdr
ch = {0: 1, 2: 3, 4: 2, 6: 4}.get(ctype)
if (w, h) != (n, n) or depth != 8 or inter != 0 or ch is None: sys.exit(1)
raw = zlib.decompress(idat); stride = w * ch; prev = bytearray(stride); p = 0
for y in range(h):
    f = raw[p]; line = bytearray(raw[p + 1:p + 1 + stride]); p += 1 + stride
    for i in range(stride):
        a = line[i - ch] if i >= ch else 0; b = prev[i]; c = prev[i - ch] if i >= ch else 0
        if f == 1: line[i] = (line[i] + a) & 255
        elif f == 2: line[i] = (line[i] + b) & 255
        elif f == 3: line[i] = (line[i] + (a + b) // 2) & 255
        elif f == 4:
            pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
            line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
    for x in range(w):
        v = (x * 7 + y * 3) % 256
        px = line[x * ch:x * ch + ch]
        if ctype in (2, 6) and (px[0], px[1], px[2]) != (v, v, v): sys.exit(1)
        if ctype in (0, 4) and px[0] != v: sys.exit(1)
    prev = line
PY
}

gui_offer() { # PART... : serves a multi-MIME selection on L1's sway until gui_stop_offers
  "$OFFER_BIN" --backend wayland --socket "$SA_RT/$SA_WL" "$@" >"$WORK/gui-offer.out" 2>&1 &
  GUI_OFFERS+=($!); PIDS+=($!)
  wait_for '^ready' "$WORK/gui-offer.out" 10
}
gui_stop_offers() { for p in ${GUI_OFFERS[@]+"${GUI_OFFERS[@]}"}; do kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; done; GUI_OFFERS=(); : >"$WORK/gui-offer.out"; }
gui_uri_paths() { wl_a wl-paste -t text/uri-list 2>/dev/null | python3 -I -c '
import sys, urllib.parse
for l in sys.stdin.read().split("\r\n"):
    if l.startswith("file://"): print(urllib.parse.unquote(l[7:]))'; }

# --- X4: pairing the two apps ---
gui_prepare() {
  REF="${REF:-$(git -C "$ROOT" rev-parse HEAD)}"
  git -C "$ROOT" fetch -q origin 2>/dev/null || true
  git -C "$ROOT" branch -r --contains "$REF" 2>/dev/null | grep -q . || fail "REF $REF is not pushed"
  if [[ -z "${SKIP_WIN_BUILD:-}" ]]; then
    log "windows: build run for the app (uniffi-bindgen-cs, dll, dotnet), alone"
    win_remote 3300 "$WORK/win-build-app.out" -- "$WIN_GUI" -Step build-app || fail "windows app build failed: $(tail -c 800 "$WORK/win-build-app.out" | tr '\n' ' ')"
  fi
  log "windows: verify-only run (health + relay TCP through the reverse tunnel)"
  win_remote 120 "$WORK/win-fwd.out" -- "$WIN_FWD" -ApiPort "$X_API_PORT" -RelayPort "$X_RELAY_PORT" || fail "tunnel verify failed: $(tail -5 "$WORK/win-fwd.out")"

  log "X4: windows app signs in (UI), shows the pairing code; the Linux app joins; SAS words compared"
  local http otp="" i
  http="$(curl -s -o "$WORK/otp-start.body" -w '%{http_code}' -X POST "$API/v1/auth/otp/start" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\"}")" || http="curl-error"
  [[ "$http" == 204 ]] || fail "otp start failed: http $http"
  for i in $(seq 1 30); do otp="$(otp_for "$EMAIL")"; [[ "$otp" =~ ^[0-9]{6}$ ]] && break; sleep 0.3; done
  [[ "$otp" =~ ^[0-9]{6}$ ]] || fail "no OTP in the backend log"
  gui_run_start pair -Step pair -Api "$API" -Relay "$RELAY" -Email "$EMAIL" -Otp "$otp" -Name "$WIN_GUI_NAME" -DeadlineSec 150
  local code; code="$(gui_sync_wait pair-code 150)"; [[ "$code" =~ ^[0-9]{6}$ ]] || fail "bad pairing code '$code' from the windows app"
  ipc L1 "{\"cmd\":\"pair-join\",\"input\":\"$code\"}" | grep -q '"ok":true' || fail "L1 pair-join rejected"
  wait_status L1 pairing.phase confirm 120
  local app_sas win_sas; app_sas="$(status_field L1 pairing.sas)"; win_sas="$(gui_sync_wait pair-sas 60)"
  [[ -n "$win_sas" && "$win_sas" == "$app_sas" ]] || fail "SAS differs: windows app '$win_sas' vs linux app '$app_sas'"
  gui_sync_put pair-sas-confirmed
  app_confirm L1
  gui_run_end
  python3 -I - "$GUI_OUT" <<'PY' || fail "windows app does not list the Linux app as Cli (see $GUI_OUT)"
import json, sys
ok = None
for line in open(sys.argv[1], errors="replace"):
    line = line.strip()
    if line.startswith("{"):
        try: o = json.loads(line)
        except ValueError: continue
        if o.get("leg") == "pair": ok = o.get("peer_listed_as_cli"); print("peer row:", str(o.get("peer_row", ""))[:200])
sys.exit(0 if ok is True else 1)
PY
  wait_status L1 pairing.phase done 30
  log "X4 ok: SAS words equal ($app_sas); the windows app lists the Linux app as Cli"
  leg x4-pair-sas-and-cli-platform app-to-win pairing 0 true "SAS equal: $app_sas; peer listed as Cli"
}

# --- X3 legs ---
gui_out_legs() { # windows app copies, the Linux app must receive
  win_measure_skew
  local nonce="$RANDOM$RANDOM" it t sent seen_ms rows setd
  gui_run_start out -Step legs -Run out -Api "$API" -Relay "$RELAY" -Name "$WIN_GUI_NAME" -Nonce "$nonce" -DeadlineSec 150
  gui_sync_wait ready 150 >/dev/null
  for it in text html png file; do
    gui_sync_put "go-$it"
    local ok=0 i
    for i in $(seq 1 400); do   # 40 s
      case "$it" in
        text) [[ "$(wl_a wl-paste --no-newline -t text/plain 2>/dev/null || true)" == "fsync-gui-$nonce-text" ]] && ok=1 ;;
        html) t="$(wl_a wl-paste --no-newline -t text/html 2>/dev/null || true)"; [[ "$t" == *"fsync-gui-$nonce-html"* ]] && ok=1 ;;
        png)  wl_a wl-paste -t image/png >"$WORK/gui-out.png" 2>/dev/null && gui_png_ok "$WORK/gui-out.png" 40 && ok=1 ;;
        file) t="$(gui_uri_paths | grep "gui-$nonce-out.bin" | head -1 || true)"; [[ -n "$t" && -f "$t" ]] && ok=1 ;;
      esac
      [[ $ok -eq 1 ]] && break
      sleep 0.1
    done
    seen_ms="$(now_ms)"
    [[ $ok -eq 1 ]] || fail "linux app never received the windows $it clip"
    gui_sync_put "got-$it"
    sent="$(gui_sync_get "sent-$it")"; [[ "$sent" =~ ^[0-9]{13}$ ]] || fail "no send timestamp for $it"
    setd="$(gui_sync_get "set-$it")"; [[ "$setd" =~ ^[0-9]{13}$ ]] && setd="clipboard write took $((setd - sent)) ms; " || setd=""
    if [[ "$it" == file ]]; then
      local want; want="$(gui_sync_get file-sha)"; [[ "$want" =~ ^[0-9a-f]{64}$ ]] || fail "no file sha from windows"
      [[ "$(sha <"$t")" == "$want" ]] || fail "file clip from windows differs (sha256)"
      LEG_MAX_MS=10000 leg "x3-$it-win-to-app" win-to-app "$it" "$((seen_ms - (sent - WIN_SKEW_MS)))" true "${setd}eager inbox; skew ${WIN_SKEW_MS} ms corrected (+-${WIN_SKEW_ERR_MS} ms)"
    else
      leg "x3-$it-win-to-app" win-to-app "$it" "$((seen_ms - (sent - WIN_SKEW_MS)))" true "${setd}skew ${WIN_SKEW_MS} ms corrected (+-${WIN_SKEW_ERR_MS} ms)"
    fi
  done
  gui_run_end
}

gui_in_auto_legs() { # the Linux app copies, the windows app (auto mode) must write its clipboard
  win_measure_skew
  local nonce="$RANDOM$RANDOM" it offer_ms recv
  gui_run_start in-auto -Step legs -Run in-auto -Api "$API" -Relay "$RELAY" -Name "$WIN_GUI_NAME" -Nonce "$nonce" -DeadlineSec 90
  for it in text html png; do
    gui_sync_wait "ready-$it" 120 >/dev/null
    case "$it" in
      text) offer_ms="$(now_ms)"; printf %s "fsync-gui-$nonce-in-text" | wl_a wl-copy ;;
      html) printf '<b>fsync-gui-%s-in-html</b>' "$nonce" >"$WORK/gui-in.html"; printf 'fsync-gui-%s-in-html' "$nonce" >"$WORK/gui-in.txt"
            offer_ms="$(now_ms)"; gui_offer text/html="$WORK/gui-in.html" text/plain="$WORK/gui-in.txt" ;;
      png)  gui_make_png "$WORK/gui-in.png" 40; offer_ms="$(now_ms)"; gui_offer image/png="$WORK/gui-in.png" ;;
    esac
    recv="$(gui_sync_wait "recv-$it" 120)"; [[ "$recv" =~ ^[0-9]{13}$ ]] || fail "no receive timestamp for $it"
    gui_stop_offers
    leg "x3-$it-app-to-win" app-to-win "$it" "$((recv - WIN_SKEW_MS - offer_ms))" true "skew ${WIN_SKEW_MS} ms corrected (+-${WIN_SKEW_ERR_MS} ms)"
  done
  gui_run_end
  python3 -I - "$GUI_OUT" <<'PY' || fail "windows app did not verify all items: $(tail -c 400 "$GUI_OUT" | tr '\n' ' ')"
import json, sys
res = None
for line in open(sys.argv[1], errors="replace"):
    line = line.strip()
    if line.startswith("{"):
        try: o = json.loads(line)
        except ValueError: continue
        if o.get("leg") == "in-auto": res = o
sys.exit(0 if res and res.get("ok") is True and all(v is True for v in res.get("items", {}).values()) else 1)
PY
}

gui_in_lazy_leg() { # file clip Linux -> Windows: virtual files, pasted by the shell copy engine
  win_measure_skew
  local nonce="$RANDOM$RANDOM" offer_ms recv want got f
  f="$WORK/home-L1/files/gui-$nonce-in.bin"; make_file "$f" 3000000; want="$(sha <"$f")"
  printf 'file://%s\r\n' "$f" >"$WORK/gui-lazy.uris"; printf 'copy\nfile://%s' "$f" >"$WORK/gui-lazy.gnome"
  gui_run_start in-lazy -Step legs -Run in-lazy -Api "$API" -Relay "$RELAY" -Name "$WIN_GUI_NAME" -Nonce "$nonce" -DeadlineSec 150
  gui_sync_wait ready-file 150 >/dev/null
  offer_ms="$(now_ms)"
  gui_offer text/uri-list="$WORK/gui-lazy.uris" x-special/gnome-copied-files="$WORK/gui-lazy.gnome"
  recv="$(gui_sync_wait recv-file 200)"; [[ "$recv" =~ ^[0-9]{13}$ ]] || fail "no receive timestamp for the file clip"
  gui_run_end
  gui_stop_offers
  got="$(python3 -I - "$GUI_OUT" <<'PY'
import json, re, sys
v = ""
for line in open(sys.argv[1], errors="replace"):
    line = line.strip()
    if line.startswith("{"):
        try: o = json.loads(line)
        except ValueError: continue
        if o.get("leg") == "in-lazy" and re.fullmatch(r"[0-9a-f]{64}", str(o.get("file_sha256", ""))): v = o["file_sha256"]
print(v)
PY
)"
  [[ "$got" == "$want" ]] || fail "file pasted on windows differs: '$got' != '$want'"
  LEG_MAX_MS=10000 leg "x3-file-app-to-win" app-to-win file "$((recv - WIN_SKEW_MS - offer_ms))" true "lazy virtual files, pasted by the shell copy engine (PasteHelper); skew ${WIN_SKEW_MS} ms corrected (+-${WIN_SKEW_ERR_MS} ms)"
}

run_windows_gui_legs() {
  gui_prepare
  gui_out_legs
  gui_in_auto_legs
  gui_in_lazy_leg
}
