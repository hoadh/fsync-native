#!/usr/bin/env bash
# Shared assertion + metric helpers for the e2e scripts. Sourced; bash 3.2 safe (macOS host).
# `fail` exits the current shell: scenarios run in a subshell, so a failure ends only that scenario.

log() { printf '[e2e] %s\n' "$*"; }
fail() { printf '[e2e] FAIL: %s\n' "$*" >&2; exit 1; }

assert_eq() { [[ "$1" == "$2" ]] || fail "${3:-assert_eq}: expected '$2', got '$1'"; }
# assert_lt VALUE LIMIT [WHAT]: floats allowed.
assert_lt() {
  python3 -c "import sys; sys.exit(0 if float('$1') < float('$2') else 1)" \
    || fail "${3:-assert_lt}: $1 is not < $2"
}
# sha_eq FILE FILE: byte comparison + sha256 of both.
sha_eq() {
  cmp "$1" "$2" || return 1
  [[ "$(shasum -a 256 "$1" | cut -d' ' -f1)" == "$(shasum -a 256 "$2" | cut -d' ' -f1)" ]]
}

# metric_add KEY VALUE: one tab-separated row in $METRIC_FILE (no-op when unset, e.g. in p0).
metric_add() { [[ -n "${METRIC_FILE:-}" ]] && printf '%s\t%s\n' "$1" "$2" >>"$METRIC_FILE"; return 0; }

# skip_scn / quarantine_scn REASON: end a scenario subshell with an explicit, recorded reason.
# Exit codes 77 (SKIP) and 78 (QUARANTINED) are read by netns/run-in-container.sh.
skip_scn() { log "SKIP: $*"; metric_add notes "$*"; exit 77; }
quarantine_scn() { log "QUARANTINED: $*"; metric_add notes "$*"; exit 78; }

# classify_path SIZE_BYTES RELAY WAN_A HOST_A: byte deltas over the file round only.
# Prints relay | lan | direct | mixed.
classify_path() {
  local size="$1" relay="$2" wan_a="$3" host_a="$4"
  if (( relay * 10 >= size * 8 )); then echo relay
  elif (( relay * 10 < size && wan_a * 10 < size && host_a >= size )); then echo lan
  elif (( relay * 10 < size && wan_a * 10 >= size * 8 )); then echo direct
  else echo mixed; fi
}
