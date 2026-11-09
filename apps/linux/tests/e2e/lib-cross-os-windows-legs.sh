#!/usr/bin/env bash
# Sourced by cross-os-with-windows.sh. Defines run_windows_legs (X2, and X5 with X5=1): Linux app <->
# fsync-cli on the Rig worker (default waso-vm), driven by SPLIT `remote run`s. Each run is one
# blocking call whose output arrives only at the end, so there is no barrier between machines:
# the sending side retries until a deadline, the receiving side waits with a deadline.
# Output of the worker is UNTRUSTED data: only whitelisted JSON fields are read, and each is
# validated (hex sha256, integers) before use. Nothing from it is executed.
# Needs from the caller: ROOT, WORK, API, RELAY, EMAIL, LEGS, leg, wl_a, wait_wl_text, accept_offer,
# offer_state, status_field, ipc, app_offer, app_confirm, otp_for, now_ms, sha, make_file, APP_BIN, APP_RT.

WIN_MACHINE="${WIN_MACHINE:-waso-vm}"
REMOTE_JS="${REMOTE_JS:-$HOME/.claude/skills/remote/scripts/remote.cjs}"
WIN_PEER="apps/windows/tests/e2e/cross-os-cli-peer.ps1"
WIN_FWD="apps/windows/tests/e2e/cross-os-port-forward.ps1"   # verify-only
WIN_NAME="devW"
# ssh target of the worker: the registry is the single source (never hardcode a user).
WIN_SSH="$(python3 -I -c 'import json,os,sys; d=json.load(open(os.path.expanduser("~/.claude/rig-machines.json"))); m=d.get("machines",d); print(m[sys.argv[1]]["ssh"])' "$WIN_MACHINE")" \
  || { echo "[e2e] no ssh target for $WIN_MACHINE in ~/.claude/rig-machines.json" >&2; exit 2; }
[[ "$WIN_SSH" =~ ^[A-Za-z0-9._@-]+$ ]] || { echo "[e2e] bad ssh target '$WIN_SSH'" >&2; exit 2; }

# win_remote TIMEOUT_S OUTFILE -- ARGS...: one `remote run` at the pinned ref; exit code = the command's.
win_remote() {
  local t="$1" out="$2"; shift 3
  win_health_gate
  node "$REMOTE_JS" run "$WIN_MACHINE" --ref "$REF" --timeout "$t" -- powershell -NoProfile -ExecutionPolicy Bypass -File "$@" >"$out" 2>&1
}

# win_health_gate: through the reverse tunnel, waso-vm's own 127.0.0.1:API must answer /health (ssh, no Rig).
win_health_gate() {
  local out
  out="$(ssh -o BatchMode=yes -o ConnectTimeout=10 "$WIN_SSH" "powershell -NoProfile -Command \"(Invoke-WebRequest http://127.0.0.1:$X_API_PORT/health -UseBasicParsing -TimeoutSec 5).StatusCode\"" 2>&1 | tr -d '\r' | tail -1)" || true
  [[ "$out" == 200 ]] || fail "health through the reverse tunnel failed on $WIN_SSH (got '$out')"
  kill -0 "$TUNNEL_PID" 2>/dev/null || fail "the ssh reverse tunnel died"
}

# win_field OUTFILE LEG FIELD KIND(hex64|int|str): last JSON line {"leg":LEG,...} from the run output.
win_field() {
  python3 -I - "$1" "$2" "$3" "$4" <<'PY'
import json, re, sys
path, leg, field, kind = sys.argv[1:5]
val = None
for line in open(path, errors="replace"):
    line = line.strip()
    if not line.startswith("{"): continue
    try: o = json.loads(line)
    except ValueError: continue
    if isinstance(o, dict) and o.get("leg") == leg and field in o: val = o[field]
ok = {"hex64": lambda v: isinstance(v, str) and re.fullmatch(r"[0-9a-f]{64}", v) is not None,
      "int": lambda v: isinstance(v, int) and not isinstance(v, bool) and 0 <= v < 10**15,
      "str": lambda v: isinstance(v, str) and len(v) < 4096 and "\n" not in v}[kind]
print(val if val is not None and ok(val) else "")
PY
}

win_prepare() { # build + pair runs; run_windows_legs calls it once
  REF="${REF:-$(git -C "$ROOT" rev-parse HEAD)}"
  git -C "$ROOT" fetch -q origin 2>/dev/null || true
  git -C "$ROOT" branch -r --contains "$REF" 2>/dev/null | grep -q . || fail "REF $REF is not pushed (remote run needs a pushed sha)"
  log "windows: build run (cargo, warm caches), alone"
  win_remote 1500 "$WORK/win-build.out" -- "$WIN_PEER" -Step build || fail "windows build failed: $(tail -5 "$WORK/win-build.out")"
  log "windows: verify-only run (health + relay TCP through the reverse tunnel)"
  win_remote 120 "$WORK/win-fwd.out" -- "$WIN_FWD" -ApiPort "$X_API_PORT" -RelayPort "$X_RELAY_PORT" \
    || fail "tunnel verify failed: $(tail -5 "$WORK/win-fwd.out")"

  # Pair run: OTP and pair code must be fresh (TTL 5 min), so both are produced right before the call.
  log "windows: login + pair (OTP from this host's backend log, app offers the code)"
  curl -fs -X POST "$API/auth/otp/start" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\"}" >/dev/null || fail "otp start failed"
  local otp="" i; for i in $(seq 1 30); do otp="$(otp_for "$EMAIL")"; [[ "$otp" =~ ^[0-9]{6}$ ]] && break; sleep 0.3; done
  [[ "$otp" =~ ^[0-9]{6}$ ]] || fail "no OTP for the windows device in the backend log"
  local code; code="$(app_offer L1)"
  win_remote 300 "$WORK/win-pair.out" -- "$WIN_PEER" -Step pair -Api "$API" -Relay "$RELAY" -Email "$EMAIL" -Otp "$otp" -PairCode "$code" -Name "$WIN_NAME" &
  WINPAIR=$!; PIDS+=("$WINPAIR")
  wait_status L1 pairing.phase confirm 120
  local app_sas; app_sas="$(status_field L1 pairing.sas)"
  app_confirm L1
  wait "$WINPAIR" || fail "windows pair run failed: $(tail -8 "$WORK/win-pair.out")"
  wait_status L1 pairing.phase done 30
  local w_sas; w_sas="$(win_field "$WORK/win-pair.out" pair sas str)"
  # Both sides print the same core `sas_words` string (app status pairing.sas, CLI event sas.words).
  [[ -n "$w_sas" && "$w_sas" == "$app_sas" ]] || fail "SAS differs: windows '$w_sas' vs app '$app_sas'"
  log "windows paired; SAS words equal on both sides: $w_sas"
  peer_online L1 "$WIN_NAME"
}

# Common peer args for a leg run.
win_leg_args() { echo -Step leg -Leg "$1" -Api "$API" -Relay "$RELAY" -Name "$WIN_NAME" -Nonce "$2" -DeadlineSec "$3" ${FORCE_RELAY:+-ForceRelay}; }

# --- the four legs; TAG labels results (x2 / x5), SIZE_B is the file size ---
win_text_to_win() { # app copies text (repeated every 8 s until the run ends); windows receives
  local tag="$1" nonce="$RANDOM$RANDOM" out="$WORK/win-$1-text-in.out" n=0 pid
  local prefix="fsync-xleg-$tag-$nonce"; declare -A ts
  # shellcheck disable=SC2046
  win_remote 300 "$out" -- "$WIN_PEER" $(win_leg_args text-in "$nonce" 240) -TextPrefix "$prefix" & pid=$!; PIDS+=("$pid")
  while kill -0 "$pid" 2>/dev/null; do
    n=$((n + 1)); ts[$n]="$(now_ms)"; printf %s "$prefix-$n" | wl_a wl-copy
    for _ in $(seq 1 40); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
  done
  wait "$pid" || fail "windows text-in leg failed: $(tail -6 "$out")"
  local txt rx; txt="$(win_field "$out" text-in text str)"; rx="$(win_field "$out" text-in recv_ts_ms int)"
  [[ "$txt" == "$prefix"-* && -n "$rx" ]] || fail "windows text-in: bad result '$txt' / '$rx'"
  local k="${txt##*-}"; [[ -n "${ts[$k]:-}" ]] || fail "windows received an unknown copy index '$k'"
  leg "$tag-text-app-to-win" app-to-win text "$((rx - ts[$k]))" true "latency across two clocks (NTP skew possible)"
}

win_text_from_win() {
  local tag="$1" nonce="$RANDOM$RANDOM" out="$WORK/win-$1-text-out.out" pid
  local prefix="fsync-xleg-$tag-$nonce"
  # shellcheck disable=SC2046
  win_remote 300 "$out" -- "$WIN_PEER" $(win_leg_args text-out "$nonce" 240) -TextPrefix "$prefix" & pid=$!; PIDS+=("$pid")
  wait_wl_text "$prefix" 280 || { wait "$pid" || true; fail "L1 never received the windows text: $(tail -6 "$out")"; }
  wait "$pid" || fail "windows text-out leg failed: $(tail -6 "$out")"
  local p0; p0="$(win_field "$out" text-out first_publish_ms int)"; [[ -n "$p0" ]] || fail "windows text-out: no first_publish_ms"
  [[ "$SEEN_TEXT" == "$prefix"* ]] || fail "unexpected text on L1: '$SEEN_TEXT'"
  leg "$tag-text-win-to-app" win-to-app text "$((SEEN_MS - p0))" true "latency across two clocks (NTP skew possible)"
}

win_file_to_win() { # app --send + pick-device (re-staged every 15 s until delivered); windows fetches
  local tag="$1" nonce="$RANDOM$RANDOM" size="$2" out="$WORK/win-$1-file-in.out" pid
  local f="$WORK/home-L1/files/xleg-$tag-$nonce.bin"; make_file "$f" "$size"; local want; want="$(sha <"$f")"
  # shellcheck disable=SC2046
  win_remote 400 "$out" -- "$WIN_PEER" $(win_leg_args file-in "$nonce" 300) & pid=$!; PIDS+=("$pid")
  local s0=0 pick
  while kill -0 "$pid" 2>/dev/null; do
    env HOME="$WORK/home-L1" XDG_RUNTIME_DIR="${APP_RT[L1]}" QT_QPA_PLATFORM=offscreen "$APP_BIN" --data-dir "${APP_DATA[L1]}" --send "$f" >/dev/null 2>&1 || true
    s0="$(now_ms)"; pick="$(ipc L1 "{\"cmd\":\"pick-device\",\"input\":\"$WIN_NAME\"}")"
    if echo "$pick" | grep -q '"ok":true'; then
      for _ in $(seq 1 75); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
    else sleep 3; fi
  done
  wait "$pid" || fail "windows file-in leg failed: $(tail -6 "$out")"
  local got rx; got="$(win_field "$out" file-in sha256 hex64)"; rx="$(win_field "$out" file-in recv_ts_ms int)"
  [[ "$got" == "$want" && -n "$rx" ]] || fail "windows file-in: sha '$got' != '$want' (or no timestamp)"
  leg "$tag-file-app-to-win" app-to-win file "$((rx - s0))" true "$size bytes; across two clocks"
}

win_file_from_win() { # windows pushes (send retries up to its deadline); app accepts over IPC
  local tag="$1" nonce="$RANDOM$RANDOM" size="$2" out="$WORK/win-$1-file-out.out" pid
  # shellcheck disable=SC2046
  win_remote 400 "$out" -- "$WIN_PEER" $(win_leg_args file-out "$nonce" 300) -SizeBytes "$size" & pid=$!; PIDS+=("$pid")
  accept_offer || { wait "$pid" || true; fail "L1 got no offer from windows: $(tail -6 "$out")"; }
  wait "$pid" || fail "windows file-out leg failed: $(tail -6 "$out")"
  local want s0 name; want="$(win_field "$out" file-out sha256 hex64)"; s0="$(win_field "$out" file-out send_start_ms int)"
  name="$(win_field "$out" file-out file_name str)"
  [[ "$name" =~ ^[A-Za-z0-9._-]+$ && -n "$want" && -n "$s0" ]] || fail "windows file-out: bad result"
  [[ "$(sha <"$WORK/home-L1/Downloads/fsync/$name")" == "$want" ]] || fail "file from windows differs (sha256)"
  leg "$tag-file-win-to-app" win-to-app file "$((DONE_MS - s0))" true "$size bytes; accept step scripted; across two clocks"
}

run_windows_legs() {
  win_prepare
  ACCEPT_WAIT=300
  FORCE_RELAY=""
  win_text_to_win x2; win_text_from_win x2
  win_file_to_win x2 3000000; win_file_from_win x2 3000000
  if [[ -n "${X5:-}" ]]; then
    log "X5: relay-forced CLI side (text + 8 MiB file)"
    FORCE_RELAY=1
    win_text_to_win x5; win_text_from_win x5
    win_file_to_win x5 8388608; win_file_from_win x5 8388608
  fi
}
