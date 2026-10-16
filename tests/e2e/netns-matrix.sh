#!/usr/bin/env bash
# Network matrix N1-N9 in Linux network namespaces, inside one privileged Docker container.
#
#   tests/e2e/netns-matrix.sh            (or: tests/e2e/run-matrix.sh --scenarios netns)
#
# Starts (all named fsync-netns-*, label fsync.e2e=netns): a docker network, a postgres, and a runner
# container built from netns/Dockerfile. The runner compiles fsync-cli, backend and the spike relay for the
# container's arch (cache volumes fsync-netns-cargo, fsync-netns-build), builds the namespaces, pairs once
# and runs the scenarios. Metrics JSON lands in docs/test-results/netns-<YYMMDD-HHMM>.json.
#
# Env: NETNS_SCENARIOS=n1,n2,... (default all), MIB or NETNS_MIB (default 64), KEEP=1 (keep logs dir),
#   QUARANTINE=n3,... (skip with an explicit note), CLEAN_CACHE=1 (also remove the cache volumes),
#   SKIP_BUILD=1, RESULTS_DIR (default docs/test-results), CLIP_MAX_S (N1 gate, default 2),
#   NETNS_STRICT=1 (a QUARANTINED gating scenario fails the run), NETNS_MAX_S (host-side bound, default 2400).
# Concurrent runs are not supported: the cache volumes fsync-netns-cargo/rustup/build are shared.
# Needs: docker (Linux VM with netns + iptables), python3. Never touches containers not named fsync-netns-*.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN="$(date +%y%m%d%H%M%S)-$$"
NET="fsync-netns-$RUN"; PG="fsync-netns-pg-$RUN"; RUNNER="fsync-netns-run-$RUN"
LABEL="fsync.e2e=netns"
OUT="$(mktemp -d)"
RESULTS_DIR="${RESULTS_DIR:-$ROOT/docs/test-results}"
MIB="${NETNS_MIB:-${MIB:-64}}"   # run-matrix.sh exports MIB=16: NETNS_MIB wins

log() { printf '[netns] %s\n' "$*"; }

# Exact names of this run only. Also used by the host-side watchdog below.
remove_run_objects() {
  docker rm -f -v "$RUNNER" "$PG" >/dev/null 2>&1 || true
  docker network rm "$NET" >/dev/null 2>&1 || true
}

cleanup() {
  local rc=$?
  if [[ -n "${WD:-}" ]]; then pkill -P "$WD" 2>/dev/null || true; kill "$WD" 2>/dev/null || true; fi
  remove_run_objects
  if [[ -n "${CLEAN_CACHE:-}" ]]; then docker volume rm fsync-netns-cargo fsync-netns-rustup fsync-netns-build >/dev/null 2>&1 || true; fi
  if [[ $rc -ne 0 || -n "${KEEP:-}" ]]; then log "logs kept in $OUT"; else rm -rf "$OUT"; fi
  exit "$rc"
}
trap cleanup EXIT INT TERM

# Host-side bound: if this script is killed hard (no trap) or the runner hangs, the privileged runner,
# postgres and network of THIS run (exact names) are removed after NETNS_MAX_S.
( sleep "${NETNS_MAX_S:-2400}"; remove_run_objects ) >/dev/null 2>&1 &
WD=$!

log "image fsync-netns:local"
docker build -q --label "$LABEL" -t fsync-netns:local "$ROOT/tests/e2e/netns" >/dev/null

docker network create --label "$LABEL" "$NET" >/dev/null
docker run -d --name "$PG" --label "$LABEL" --network "$NET" -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=fsync \
  postgres:17 >/dev/null
for _ in $(seq 1 60); do
  docker exec "$PG" pg_isready -U postgres -d fsync >/dev/null 2>&1 && break
  sleep 0.5
done
sleep 1
docker exec "$PG" pg_isready -U postgres -d fsync >/dev/null || { log "postgres not ready"; exit 1; }

# The tested commit; "-dirty" when files other than past results differ from it.
SHA="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
if [[ -n "$(git -C "$ROOT" status --porcelain -- . ':(exclude)docs/test-results' 2>/dev/null)" ]]; then SHA="$SHA-dirty"; fi
rc=0
docker run --name "$RUNNER" --label "$LABEL" --privileged --network "$NET" \
  -v "$ROOT":/src:ro -v fsync-netns-cargo:/cargo -v fsync-netns-rustup:/usr/local/rustup -v fsync-netns-build:/build -v "$OUT":/out \
  -e MIB="$MIB" -e NETNS_SCENARIOS="${NETNS_SCENARIOS:-n1,n2,n5,n3,n3b,n4,n7,n9,n6,n8,sec}" \
  -e QUARANTINE="${QUARANTINE:-}" -e SKIP_BUILD="${SKIP_BUILD:-}" -e CLIP_MAX_S="${CLIP_MAX_S:-2}" -e NETNS_STRICT="${NETNS_STRICT:-0}" \
  -e N1_EXPECT_PATH="${N1_EXPECT_PATH:-lan}" -e RUN_ID="$RUN" -e GIT_SHA="$SHA" \
  -e DATABASE_URL="postgres://postgres:e2e@$PG:5432/fsync" \
  fsync-netns:local bash /src/tests/e2e/netns/run-in-container.sh 2>&1 | tee "$OUT/runner.log" || rc=${PIPESTATUS[0]}
log "wall ${SECONDS}s since start (incl. image/pg/build); matrix time is in the JSON"

if [[ -f "$OUT/netns-result.json" ]]; then
  mkdir -p "$RESULTS_DIR"
  dest="$RESULTS_DIR/netns-$(date +%y%m%d-%H%M).json"
  python3 -I -c "import json,sys; json.load(open(sys.argv[1]))" "$OUT/netns-result.json"
  cp "$OUT/netns-result.json" "$dest"
  log "metrics: $dest"
fi
exit "$rc"
