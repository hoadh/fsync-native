#!/usr/bin/env bash
# N7: B goes away. A must see B offline within 60 s; a clip published while B is away is NOT
# delivered when B returns (no store-and-forward); a new clip after B returns is.
scenario_n7() {
  use_pair lan
  local dir="$WORK/round-n7"; mkdir -p "$dir"
  cli B clip watch --json --count 0 --timeout 300 >"$dir/b-up.out" 2>"$dir/b-up.err" &
  local bp=$!; track "$bp"
  wait_for '"event":"ready"' "$dir/b-up.out" 60
  poll_b_online true 30
  cli_kill B; wait "$bp" 2>/dev/null || true
  local t0=$SECONDS
  poll_b_online false 60
  metric_add notes "B seen offline after $((SECONDS - t0))s"
  if cli A clip put --text "while-offline" --wait 5 >"$dir/put-off.out" 2>"$dir/put-off.err"; then
    fail "clip put reported delivery while B was stopped"
  fi
  cli B clip watch --json --count 1 --timeout 10 >"$dir/b-back.out" 2>"$dir/b-back.err" &
  local w=$!; track "$w"
  wait_for '"event":"ready"' "$dir/b-back.out" 60
  wait "$w" || true
  grep -q '"event":"clip"' "$dir/b-back.out" && fail "stale clip delivered after B returned"
  CLIP_MAX_S=5 clip_round A B n7b
}

# poll_b_online true|false SECS: A's `devices --json` until devB's online flag matches.
poll_b_online() {
  local want="$1" secs="$2" end=$((SECONDS + $2)) v=""
  while [[ $SECONDS -lt $end ]]; do
    cli A devices --json >"$WORK/devs.json" 2>/dev/null || true
    v="$(python3 -I -c "
import json,sys
try: d=json.loads(open('$WORK/devs.json').read().strip().splitlines()[-1])
except Exception: d=[]
print(next((str(x['online']).lower() for x in d if x['name']=='devB'),'none'))")"
    [[ "$v" == "$want" ]] && return 0
  done
  fail "devB online=$v after ${secs}s, wanted $want"
}
