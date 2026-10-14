#!/usr/bin/env bash
# Phase 15 harness entry point: runs the existing e2e scenarios over a matrix of relay configs
# and prints one summary table. Exit 1 if any cell failed.
#
#   tests/e2e/run-matrix.sh [--scenarios a,b] [--relays x,y] [--list]
#
# Scenarios (scripts in this directory):
#   p0     p0-exit-demo.sh        two CLI devices: pair, clip, file direct + forced relay (honors E2E_RELAY_URL)
#   stack  compose-stack-sync.sh  production compose stack incl. its own relay (relay axis does not apply: run once)
# Relays: local (the scenario's own relay, default), us (relay-fsync.10z.one), sg (relay-sg-fsync.10z.one),
#   or any http(s):// URL. Env: MIB (file size, default 16 here), CELL_TIMEOUT_S (default 1800),
#   OUT_DIR (logs, default a temp dir). The "direct" round of p0 needs both devices on one LAN.
# Needs what the scenarios need: docker, cargo, python3. Not a CI job: the hosted relays are production.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# bash 3.2 (macOS /bin/bash) has no associative arrays: plain functions.
script_of() { case "$1" in p0) echo p0-exit-demo.sh;; stack) echo compose-stack-sync.sh;; esac; }
url_of() { case "$1" in us) echo https://relay-fsync.10z.one;; sg) echo https://relay-sg-fsync.10z.one;; http://*|https://*) echo "$1";; esac; }
scenarios="p0" relays="local"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --scenarios) scenarios="${2:?}"; shift 2;;
    --relays) relays="${2:?}"; shift 2;;
    --list) echo "scenarios: p0 stack"; echo "relays: local us sg or an http(s) URL"; exit 0;;
    *) sed -n 2,5p "$0" >&2; exit 2;;
  esac
done
export MIB="${MIB:-16}"
out="${OUT_DIR:-$(mktemp -d)}"; mkdir -p "$out"
limit="${CELL_TIMEOUT_S:-1800}"
rows=(); failed=0

run_cell() { # scenario relay -> appends a summary row
  local s="$1" r="$2" log="$out/$1-${2//[^A-Za-z0-9.-]/_}.log" url="" t0 rc script
  script="$(script_of "$s")"
  [[ -n "$script" ]] || { rows+=("$s|$r|FAIL|0|unknown scenario"); failed=1; return; }
  if [[ "$r" != local ]]; then
    url="$(url_of "$r")"; [[ -n "$url" ]] || { rows+=("$s|$r|FAIL|0|unknown relay"); failed=1; return; }
  fi
  t0=$SECONDS
  # Own process group (set -m) so the watchdog can signal the scenario and everything it spawned.
  set -m
  E2E_RELAY_URL="$url" "$HERE/$script" >"$log" 2>&1 &
  local pid=$!
  ( sleep "$limit" & wait $!; kill -TERM -- "-$pid" 2>/dev/null ) >/dev/null 2>&1 &
  local wd=$!
  set +m
  wait "$pid"; rc=$?
  pkill -P "$wd" 2>/dev/null; kill "$wd" 2>/dev/null; wait "$wd" 2>/dev/null
  local note; note="$(grep -E 'FAIL|latency_s=' "$log" | tail -1 | cut -c1-90)"
  if [[ $rc -eq 0 ]]; then rows+=("$s|$r|PASS|$((SECONDS - t0))s|$note")
  else failed=1; rows+=("$s|$r|FAIL|$((SECONDS - t0))s|rc=$rc ${note}"); fi
}

IFS=, read -r -a sc <<<"$scenarios"; IFS=, read -r -a rl <<<"$relays"
for s in "${sc[@]}"; do
  if [[ "$s" == stack ]]; then run_cell stack local; continue; fi
  for r in "${rl[@]}"; do run_cell "$s" "$r"; done
done

printf '\n%-8s %-34s %-5s %-7s %s\n' SCENARIO RELAY RESULT TIME NOTE
for row in "${rows[@]}"; do IFS='|' read -r a b c d e <<<"$row"; printf '%-8s %-34s %-5s %-7s %s\n' "$a" "$b" "$c" "$d" "$e"; done
echo "logs: $out"
exit "$failed"
