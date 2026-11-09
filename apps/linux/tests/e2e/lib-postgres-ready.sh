#!/usr/bin/env bash
# Sourced. wait_pg_ready CONTAINER [SECS]: the postgres image starts a temporary server for its
# init scripts, stops it and starts the real one, so the first "ready" is a false positive. Wait
# for the SECOND "ready to accept connections" log line, then for 3 consecutive pg_isready
# successes. Calls `fail` (from assert.sh) on timeout.
wait_pg_ready() {
  local name="$1" secs="${2:-120}" i ok=0
  for i in $(seq 1 $((secs * 2))); do
    if [[ "$(docker logs "$name" 2>&1 | grep -c 'ready to accept connections')" -ge 2 ]]; then
      if docker exec "$name" pg_isready -U postgres -d fsync >/dev/null 2>&1; then
        ok=$((ok + 1)); [[ $ok -ge 3 ]] && return 0
      else ok=0; fi
    fi
    sleep 0.5
  done
  fail "postgres not ready after ${secs}s"
}
