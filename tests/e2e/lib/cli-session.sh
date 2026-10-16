#!/usr/bin/env bash
# CLI session helpers shared by p0-exit-demo.sh and the netns matrix. Sourced; bash 3.2 safe.
# Needs: assert.sh, and CLI, API, RELAY, WORK set by the caller; backend log at $WORK/backend.log.
# Optional: CLI_NS_<NAME> (network namespace a device runs in), EXTRA (global flags array),
# PIDS (array of background pids the caller cleans), PIDFILE (pids appended for subshell users).

wait_for() { # PATTERN FILE [SECS]: polls `grep -q`
  local pattern="$1" file="$2" secs="${3:-60}" i=0
  until grep -q "$pattern" "$file" 2>/dev/null; do
    sleep 0.2; i=$((i + 1))
    [[ $i -lt $((secs * 5)) ]] || fail "timeout waiting for '$pattern' in $file ($(cat "${file%.out}.err" 2>/dev/null | tail -2))"
  done
}

# jfield FILE EVENT FIELD: field of the last JSON line with "event":EVENT.
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

track() { PIDS+=("$1"); [[ -n "${PIDFILE:-}" ]] && echo "$1" >>"$PIDFILE"; return 0; }

# cli NAME ARGS...: global flags first, then the subcommand. Runs in CLI_NS_NAME when set.
EXTRA=()
cli() {
  local n="$1"; shift
  local v="CLI_NS_$n" ns=""
  ns="${!v:-}"
  local pre=()
  [[ -n "$ns" ]] && pre=(ip netns exec "$ns")
  ${pre[@]+"${pre[@]}"} "$CLI" --data "$WORK/$n" --api "$API" --relay "$RELAY" --insecure-http --name "dev$n" \
    ${EXTRA[@]+"${EXTRA[@]}"} "$@"
}

otp_for() { # newest dev OTP logged for the address
  sed 's/\x1b\[[0-9;]*m//g' "$WORK/backend.log" | grep "dev otp" | grep "to=\"$1\"" | tail -1 \
    | sed -n 's/.*code="\([0-9]*\)".*/\1/p'
}

login() { # NAME EMAIL
  local n="$1" email="$2" code="" i
  cli "$n" login --email "$email" --send-only >/dev/null
  for i in $(seq 1 20); do code="$(otp_for "$email")"; [[ -n "$code" ]] && break; sleep 0.3; done
  [[ -n "$code" ]] || fail "no OTP in backend log for $n"
  cli "$n" login --email "$email" --code "$code" | tee "$WORK/login-$n.json"
}

# pair OFFERER JOINER: offerer shows a code, joiner enters it, SAS must match.
pair() {
  local o="$1" j="$2"
  cli "$o" pair offer --yes >"$WORK/pair-$o.out" 2>"$WORK/pair-$o.err" &
  local op=$!; track "$op"
  wait_for '"event":"offer"' "$WORK/pair-$o.out" 30
  local code; code="$(jfield "$WORK/pair-$o.out" offer code)"
  [[ -n "$code" ]] || fail "no pairing code"
  cli "$j" pair join "$code" --yes >"$WORK/pair-$j.out"
  wait "$op" || fail "$o pair offer failed: $(cat "$WORK/pair-$o.err")"
  local sa sb
  sa="$(jfield "$WORK/pair-$j.out" sas words)"; sb="$(jfield "$WORK/pair-$o.out" sas words)"
  [[ -n "$sa" && "$sa" == "$sb" ]] || fail "SAS differs: '$sa' vs '$sb'"
  grep -q '"event":"paired"' "$WORK/pair-$j.out" || fail "$j did not report paired"
  grep -q '"event":"paired"' "$WORK/pair-$o.out" || fail "$o did not report paired"
  log "paired, SAS words: $sa"
}

# clip_round FROM TO LABEL: clip text FROM -> TO. Sets CLIP_LAT_S. Gate: CLIP_MAX_S (default 5).
clip_round() {
  local from="$1" to="$2" label="$3" dir="$WORK/round-$3"
  mkdir -p "$dir"
  log "[$label] clipboard $from -> $to"
  cli "$to" clip watch --json --count 1 --timeout 90 >"$dir/watch.out" 2>"$dir/watch.err" &
  local w=$!; track "$w"
  wait_for '"event":"ready"' "$dir/watch.out" 60
  cli "$from" clip put --text "hello from $from ($label)" --wait 60 >"$dir/put.out" 2>"$dir/put.err" \
    || fail "[$label] clip put failed: $(cat "$dir/put.err")"
  wait "$w" || fail "[$label] clip watch failed: $(cat "$dir/watch.err")"
  [[ "$(jfield "$dir/watch.out" clip text)" == "hello from $from ($label)" ]] || fail "[$label] clip text mismatch"
  local p0 r1
  p0="$(jfield "$dir/put.out" published first_publish_ms)"; r1="$(jfield "$dir/watch.out" clip ts_ms)"
  [[ -n "$p0" && -n "$r1" ]] || fail "[$label] missing clip timestamps"
  CLIP_LAT_S="$(python3 -c "print(round(($r1-$p0)/1000, 2))")"
  printf '%s clip %s->%s latency_s=%s (settle_ms=%s, attempts=%s)\n' "$label" "$from" "$to" "$CLIP_LAT_S" \
    "$(jfield "$dir/put.out" published settle_ms)" "$(jfield "$dir/put.out" published attempts)" | tee -a "${RESULTS:-/dev/null}"
  metric_add clip_latency_s "$CLIP_LAT_S"
  assert_lt "$CLIP_LAT_S" "${CLIP_MAX_S:-5}" "[$label] clip latency (s)"
}

# file_round FROM TO LABEL [MIB]: MIB file FROM -> TO, hash checked. Sets FILE_VIA, FILE_SECS, FILE_MIB_S.
# Caller samples counters around it. Does not judge the path.
file_round() {
  local from="$1" to="$2" label="$3" mib="${4:-$MIB}" dir="$WORK/round-$3"
  mkdir -p "$dir"
  log "[$label] ${mib} MiB file $from -> $to"
  local src="$dir/src.bin"
  dd if=/dev/urandom of="$src" bs=1048576 count="$mib" 2>/dev/null
  cli "$to" fetch --out "$dir/recv" --timeout 900 >"$dir/fetch.out" 2>"$dir/fetch.err" &
  local f=$!; track "$f"
  wait_for '"event":"ready"' "$dir/fetch.out" 60
  cli "$from" send "dev$to" "$src" --wait 60 >"$dir/send.out" 2>"$dir/send.err" \
    || fail "[$label] send failed: $(cat "$dir/send.err")"
  wait "$f" || fail "[$label] fetch failed: $(cat "$dir/fetch.err")"
  local got; got="$(jfield "$dir/fetch.out" fetched path)"
  [[ -f "$got" ]] || fail "[$label] received file missing"
  sha_eq "$src" "$got" || fail "[$label] received file differs"
  FILE_VIA="$(jfield "$dir/send.out" sent via)"
  FILE_SECS="$(jfield "$dir/fetch.out" fetched secs)"
  FILE_MIB_S="$(jfield "$dir/fetch.out" fetched mib_s)"
  printf '%s file %s MiB sha ok send_secs=%s recv_secs=%s recv_mib_s=%s via=%s\n' "$label" "$mib" \
    "$(jfield "$dir/send.out" sent secs)" "$FILE_SECS" "$FILE_MIB_S" "$FILE_VIA" | tee -a "${RESULTS:-/dev/null}"
  metric_add sha_ok true; metric_add file_secs "$FILE_SECS"; metric_add mib_s "$FILE_MIB_S"
  rm -rf "$src" "$dir/recv"
}
