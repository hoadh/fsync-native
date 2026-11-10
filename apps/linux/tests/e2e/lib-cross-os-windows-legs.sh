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

# win_measure_skew: WIN_SKEW_MS = (windows clock - this host's clock), WIN_SKEW_ERR_MS = +- uncertainty.
# One ssh session streams the worker's clock every ~20 ms (no per-sample ssh handshake or PowerShell start-up:
# those made the old one-shot probe overestimate the skew by 0.3-0.5 s, hence latencies below zero). Each
# line is stamped on arrival here: t_win - t_arrival = skew - one_way_delay, so the MAXIMUM over ~130 samples
# is the skew minus the smallest delay seen. That delay is bounded by the connection's own smoothed TCP round
# trip (ss rtt, same path), which with a few ms of pipe jitter is the reported error.
# Cross-clock leg latencies are corrected with it (the VM clock runs seconds ahead).
WIN_SKEW_MS=0; WIN_SKEW_ERR_MS=0
win_measure_skew() {
  local ip fd pid n=0 w t d best="" minrtt="" r
  ip="$(getent ahostsv4 "$(ssh -G "$WIN_SSH" | awk '/^hostname /{print $2}')" | awk 'NR==1{print $1}')"
  exec {fd}< <(ssh -o BatchMode=yes -o ConnectTimeout=10 "$WIN_SSH" 'powershell -NoProfile -Command "1..150 | ForEach-Object { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); Start-Sleep -Milliseconds 20 }"' 2>/dev/null)
  pid=$!
  while read -r -t 15 -u "$fd" w; do
    t="${EPOCHREALTIME/./}"; t=$((t / 1000))
    w="${w//$'\r'/}"
    [[ "$w" =~ ^[0-9]{13}$ ]] || continue
    n=$((n + 1)); [[ $n -gt 20 ]] || continue   # the first samples carry the start-up
    d=$((w - t))
    if [[ -z "$best" || $d -gt $best ]]; then best=$d; fi
    if [[ $n -eq 100 && -n "$ip" ]]; then
      for r in $(ss -tin dst "$ip" 2>/dev/null | grep -o ' rtt:[0-9.]*' | cut -d: -f2); do
        r="${r%%.*}"; [[ -z "$minrtt" || $r -lt $minrtt ]] && minrtt="$r"
      done
    fi
  done
  exec {fd}<&-
  kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true
  [[ -n "$best" && $n -gt 60 ]] || fail "could not read the windows clock stream over ssh ($n samples)"
  [[ -n "$minrtt" ]] || { minrtt=100; log "windows clock skew: no ss rtt, assuming 100 ms"; }
  WIN_SKEW_ERR_MS=$((minrtt + 6)); WIN_SKEW_MS="$best"
  log "windows clock skew: ${WIN_SKEW_MS} ms (+-${WIN_SKEW_ERR_MS} ms; ${n} samples, tcp rtt ${minrtt} ms)"
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

  # Pair run: the OTP must be fresh (TTL 5 min), so it is produced right before the call.
  log "windows: login + pair (OTP from this host's backend log; windows offers, app joins)"
  local http; http="$(curl -s -o "$WORK/otp-start.body" -w '%{http_code}' -X POST "$API/v1/auth/otp/start" -H 'content-type: application/json' -d "{\"email\":\"$EMAIL\"}")" || http="curl-error"
  [[ "$http" == 204 ]] || fail "otp start failed: http $http body '$(head -c 200 "$WORK/otp-start.body" 2>/dev/null)'"
  local otp="" i; for i in $(seq 1 30); do otp="$(otp_for "$EMAIL")"; [[ "$otp" =~ ^[0-9]{6}$ ]] && break; sleep 0.3; done
  [[ "$otp" =~ ^[0-9]{6}$ ]] || fail "no OTP for the windows device in the backend log"
  # A device of an EXISTING account cannot `pair join` (core/fsync-cli ctx.rs: "not paired yet"): the
  # windows CLI OFFERS and the already paired app joins. The run's output only arrives at its end, so
  # the offer event is read over plain ssh from the file the peer writes (read-only probing).
  local offer_path="(Join-Path \$env:TEMP 'fsync-cross-os\\pair-offer.out')"   # PowerShell expression, expanded on the worker
  ssh -o BatchMode=yes -o ConnectTimeout=10 "$WIN_SSH" "powershell -NoProfile -Command \"Remove-Item -Force -ErrorAction SilentlyContinue $offer_path\"" >/dev/null 2>&1 || true
  win_remote 300 "$WORK/win-pair.out" -- "$WIN_PEER" -Step pair -Api "$API" -Relay "$RELAY" -Email "$EMAIL" -Otp "$otp" -Name "$WIN_NAME" -DeadlineSec 200 &
  WINPAIR=$!; PIDS+=("$WINPAIR")
  local code=""
  for i in $(seq 1 240); do
    code="$(ssh -o BatchMode=yes -o ConnectTimeout=10 "$WIN_SSH" "powershell -NoProfile -Command \"Get-Content -ErrorAction SilentlyContinue $offer_path\"" 2>/dev/null \
      | python3 -I -c '
import json, re, sys
for line in sys.stdin:
    try: o = json.loads(line.strip())
    except ValueError: continue
    if isinstance(o, dict) and o.get("event") == "offer" and re.fullmatch(r"[0-9]{6}", str(o.get("code", ""))): print(o["code"])' | tail -1)" || true   # pipefail + set -e: a missing file (ssh exit 1) must just retry
    [[ -n "$code" ]] && break
    kill -0 "$WINPAIR" 2>/dev/null || { wait "$WINPAIR" || true; fail "windows pair run ended before offering a code: $(tail -8 "$WORK/win-pair.out")"; }
    sleep 1
  done
  [[ -n "$code" ]] || fail "no pairing code from the windows device within 240 s"
  ipc L1 "{\"cmd\":\"pair-join\",\"input\":\"$code\"}" | grep -q '"ok":true' || fail "L1 pair-join rejected"
  # Stop early when the windows run already ended.
  for i in $(seq 1 600); do
    [[ "$(status_field L1 pairing.phase)" == confirm ]] && break
    kill -0 "$WINPAIR" 2>/dev/null || { wait "$WINPAIR" || true; fail "windows pair run ended before the SAS prompt: $(tail -8 "$WORK/win-pair.out")"; }
    sleep 0.2
  done
  if [[ "$(status_field L1 pairing.phase)" != confirm ]]; then
    ssh -o BatchMode=yes -o ConnectTimeout=10 "$WIN_SSH" "powershell -NoProfile -Command \"Get-Content -ErrorAction SilentlyContinue $offer_path\"" >"$WORK/win-offer-file.txt" 2>&1 || true
    fail "L1 never reached the SAS prompt (L1 phase '$(status_field L1 pairing.phase)', message '$(status_field L1 pairing.message)'; windows offer file: $(tr '\n' ' ' <"$WORK/win-offer-file.txt" | cut -c1-300))"
  fi
  local app_sas; app_sas="$(status_field L1 pairing.sas)"
  app_confirm L1
  wait "$WINPAIR" || fail "windows pair run failed: $(tail -8 "$WORK/win-pair.out")"
  wait_status L1 pairing.phase done 30
  local w_sas; w_sas="$(win_field "$WORK/win-pair.out" pair sas str)"
  # Both sides print the same core `sas_words` string (app status pairing.sas, CLI event sas.words).
  [[ -n "$w_sas" && "$w_sas" == "$app_sas" ]] || fail "SAS differs: windows '$w_sas' vs app '$app_sas'"
  log "windows paired; SAS words equal on both sides: $w_sas"
  # No online check here: the windows CLI exits after pairing; each leg run brings it online again.
}

# Common peer args for a leg run.
win_leg_args() { echo -Step leg -Leg "$1" -Api "$API" -Relay "$RELAY" -Name "$WIN_NAME" -Nonce "$2" -DeadlineSec "$3" ${FORCE_RELAY:+-ForceRelay}; }

# --- the four legs; TAG labels results (x2 / x5), SIZE_B is the file size ---
win_text_to_win() { # app copies text (repeated every 8 s until the run ends); windows receives
  win_measure_skew
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
  LEG_SKEW="$WIN_SKEW_MS" LEG_ERR_MS="$WIN_SKEW_ERR_MS" leg "$tag-text-app-to-win" app-to-win text "$((rx - WIN_SKEW_MS - ts[$k]))" true "cross-clock, skew ${WIN_SKEW_MS} ms corrected (+-${WIN_SKEW_ERR_MS} ms)"
}

win_text_from_win() {
  win_measure_skew
  local tag="$1" nonce="$RANDOM$RANDOM" out="$WORK/win-$1-text-out.out" pid
  local prefix="fsync-xleg-$tag-$nonce"
  # shellcheck disable=SC2046
  win_remote 300 "$out" -- "$WIN_PEER" $(win_leg_args text-out "$nonce" 240) -TextPrefix "$prefix" & pid=$!; PIDS+=("$pid")
  wait_wl_text "$prefix" 280 || { wait "$pid" || true; fail "L1 never received the windows text: $(tail -6 "$out")"; }
  wait "$pid" || fail "windows text-out leg failed: $(tail -6 "$out")"
  local p0; p0="$(win_field "$out" text-out first_publish_ms int)"; [[ -n "$p0" ]] || fail "windows text-out: no first_publish_ms"
  [[ "$SEEN_TEXT" == "$prefix"* ]] || fail "unexpected text on L1: '$SEEN_TEXT'"
  LEG_SKEW="$WIN_SKEW_MS" LEG_ERR_MS="$WIN_SKEW_ERR_MS" leg "$tag-text-win-to-app" win-to-app text "$((SEEN_MS - p0 + WIN_SKEW_MS))" true "cross-clock, skew ${WIN_SKEW_MS} ms corrected (+-${WIN_SKEW_ERR_MS} ms)"
}

win_file_to_win() { # app --send + pick-device (re-staged every 15 s until delivered); windows fetches
  win_measure_skew
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
  LEG_MAX_MS=10000 LEG_SKEW="$WIN_SKEW_MS" LEG_ERR_MS="$WIN_SKEW_ERR_MS" leg "$tag-file-app-to-win" app-to-win file "$((rx - WIN_SKEW_MS - s0))" true "$size bytes; cross-clock, skew ${WIN_SKEW_MS} ms corrected (+-${WIN_SKEW_ERR_MS} ms)"
}

win_file_from_win() { # windows pushes (send retries up to its deadline); app accepts over IPC
  win_measure_skew
  local tag="$1" nonce="$RANDOM$RANDOM" size="$2" out="$WORK/win-$1-file-out.out" pid
  # shellcheck disable=SC2046
  win_remote 400 "$out" -- "$WIN_PEER" $(win_leg_args file-out "$nonce" 300) -SizeBytes "$size" & pid=$!; PIDS+=("$pid")
  accept_offer || { wait "$pid" || true; fail "L1 got no offer from windows: $(tail -6 "$out")"; }
  wait "$pid" || fail "windows file-out leg failed: $(tail -6 "$out")"
  local want s0 name; want="$(win_field "$out" file-out sha256 hex64)"; s0="$(win_field "$out" file-out send_start_ms int)"
  name="$(win_field "$out" file-out file_name str)"
  [[ "$name" =~ ^[A-Za-z0-9._-]+$ && -n "$want" && -n "$s0" ]] || fail "windows file-out: bad result"
  [[ "$(sha <"$WORK/home-L1/Downloads/fsync/$name")" == "$want" ]] || fail "file from windows differs (sha256)"
  LEG_MAX_MS=10000 LEG_SKEW="$WIN_SKEW_MS" LEG_ERR_MS="$WIN_SKEW_ERR_MS" leg "$tag-file-win-to-app" win-to-app file "$((DONE_MS - s0 + WIN_SKEW_MS))" true "$size bytes; accept step scripted; cross-clock, skew ${WIN_SKEW_MS} ms corrected (+-${WIN_SKEW_ERR_MS} ms)"
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
