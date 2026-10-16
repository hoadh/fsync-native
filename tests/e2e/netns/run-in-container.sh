#!/usr/bin/env bash
# Runs inside the privileged runner container (started by ../netns-matrix.sh). Builds the binaries
# (cache volumes), wires the namespaces, starts backend + relay, pairs A and B once, then runs the
# scenarios in order and writes the metrics JSON to /out.
#
# Env: MIB, NETNS_SCENARIOS (csv, default all), DATABASE_URL, RUN_ID, GIT_SHA, SKIP_BUILD=1, QUARANTINE (csv ids), NETNS_STRICT=1 (quarantined gating scenario fails the run)
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E="$(cd "$HERE/.." && pwd)"
WORK=/work; BIN_DIR=/build/bin; MIB="${MIB:-64}"; CLIP_MAX_S="${CLIP_MAX_S:-2}"
RUN_ID="${RUN_ID:-local}"
PIDS=(); RESULTS="$WORK/results.txt"
export WORK BIN_DIR MIB CLIP_MAX_S
# shellcheck disable=SC1091
. "$E2E/lib/assert.sh"; . "$E2E/lib/cli-session.sh"; . "$E2E/lib/netns-topology.sh"; . "$E2E/lib/netns-services.sh"

rm -rf "$WORK"; mkdir -p "$WORK" "$BIN_DIR/spike"; : >"$RESULTS"
T0=$SECONDS

cleanup() {
  local p
  for p in ${PIDS[@]+"${PIDS[@]}"} $(cat "$WORK"/*.pid 2>/dev/null); do kill "$p" 2>/dev/null || true; done
  topo_destroy
  cp -R "$WORK" /out/work 2>/dev/null || true
}
trap cleanup EXIT

if [[ -z "${SKIP_BUILD:-}" ]]; then
  log "build (release, cached)"
  B0=$SECONDS
  (cd /src && CARGO_TARGET_DIR=/build/ws cargo build --release --locked -p fsync-backend -p fsync-cli) || fail "workspace build"
  (cd /src/spikes/iroh-discovery && CARGO_TARGET_DIR=/build/spike cargo build --release --locked) || fail "spike build"
  log "build took $((SECONDS - B0))s"
fi
cp /build/ws/release/fsync-backend /build/ws/release/fsync-cli "$BIN_DIR/"
cp /build/spike/release/iroh-discovery-spike "$BIN_DIR/spike/"
CLI="$BIN_DIR/fsync-cli"
BUILD_S=$((SECONDS - T0)); T0=$SECONDS   # the 15 minute budget counts from here

log "topology + services"
echo 'kernel:' "$(uname -r)"; iptables --version
topo_build || fail "topology"
: >"$WORK/backend.log"
backend_start
relay_start
API="http://$API_ADDR"; RELAY="http://100.64.0.2:$RELAY_PORT"
use_pair lan

EMAIL="e2e-$$@example.test"
log "login A (genesis) and B, pair once"
login A "$EMAIL" >/dev/null; grep -q '"account":"created"' "$WORK/login-A.json" || fail "A should create the account"
login B "$EMAIL" >/dev/null; grep -q '"account":"needs_pairing"' "$WORK/login-B.json" || fail "B should need pairing"
pair B A

# id file-stem gating. Order matters: N6 and N8 mutate state, N8 destroys the pairing and is last.
SCN="n1:n1-lan:1 n2:n2-wan-down:1 n5:n5-force-relay:1 n3:n3-full-cone:1 n3b:n3b-stateful-punch:1 n4:n4-symmetric:1 n7:n7-offline-peer:1 n9:n9-backend-down:1 n6:n6-readdress:0 n8:n8-reset-trust:1 sec:sec-unpaired-dial:0"
WANT=",${NETNS_SCENARIOS:-n1,n2,n5,n3,n3b,n4,n7,n9,n6,n8,sec},"
QUAR=",${QUARANTINE:-},"
ROWS="$WORK/rows"; : >"$ROWS"
overall=0; QGATING=""
for entry in $SCN; do
  id="${entry%%:*}"; rest="${entry#*:}"; stem="${rest%%:*}"; gating="${rest##*:}"
  [[ "$WANT" == *",$id,"* ]] || continue
  log "=== $id ($stem) gating=$gating"
  export METRIC_FILE="$WORK/m-$id.tsv" PIDFILE="$WORK/pids-$id" RESULTS
  : >"$METRIC_FILE"; : >"$PIDFILE"
  s0=$SECONDS
  if [[ "$QUAR" == *",$id,"* ]]; then
    printf 'notes\tquarantined by QUARANTINE env\n' >>"$METRIC_FILE"; rc=78
  else
    # shellcheck disable=SC1090
    ( set -uo pipefail; EXTRA=(); PIDS=(); . "$E2E/scenarios/$stem.sh"; "scenario_$id" ) >"$WORK/scn-$id.log" 2>&1 &
    spid=$!
    # watchdog: a hung scenario fails after SCN_TIMEOUT_S instead of eating the run
    ( sleep "${SCN_TIMEOUT_S:-300}"; kill -0 "$spid" 2>/dev/null || exit 0; echo "[e2e] FAIL: scenario $id timed out" >>"$WORK/scn-$id.log"; kill -TERM "$spid"; cli_kill ) 2>/dev/null &
    wd=$!
    wait "$spid"; rc=$?
    pkill -P "$wd" 2>/dev/null; kill "$wd" 2>/dev/null; wait "$wd" 2>/dev/null
    [[ $rc -gt 128 ]] && rc=1
  fi
  while read -r p; do kill "$p" 2>/dev/null || true; done <"$PIDFILE"
  cli_kill
  backend_up || { log "backend was down after $id: restarting"; backend_start; }
  wan_up r-lan; wan_up r-a; wan_up r-b
  nat_mode r-lan masq; nat_mode r-a masq; nat_mode r-b masq; use_pair lan
  hosts_reset   # a scenario that moved a host address (N6) must not leak into the next one
  case "$rc" in 0) res=PASS;; 77) res=SKIP;; 78) res=QUARANTINED;; *) res=FAIL;; esac
  [[ $res == FAIL && $gating == 1 ]] && overall=1
  [[ $res == QUARANTINED && $gating == 1 ]] && QGATING="${QGATING:+$QGATING }$id"
  printf '%s\t%s\t%s\t%s\n' "$id" "$res" "$gating" "$((SECONDS - s0))" >>"$ROWS"
  log "=== $id -> $res ($((SECONDS - s0))s)"; tail -3 "$WORK/scn-$id.log" | sed 's/^/    /'
done

DUR=$((SECONDS - T0))
STRICT_FLAG="${NETNS_STRICT:-0}"
python3 -I "$HERE/metrics-write.py" "$WORK" "/out/netns-result.json" "$RUN_ID" "${GIT_SHA:-unknown}" "$MIB" "$DUR" "$BUILD_S" "$STRICT_FLAG" || overall=1
echo "---- scenarios (wall ${DUR}s, build ${BUILD_S}s)"
cat "$ROWS"
count_of() { awk -F'\t' -v r="$1" '$2 == r {n++} END {print n + 0}' "$ROWS"; }
echo "---- summary: PASS=$(count_of PASS) FAIL=$(count_of FAIL) QUARANTINED=$(count_of QUARANTINED) SKIP=$(count_of SKIP)"
if [[ -n "$QGATING" ]]; then
  echo "!!!! WARNING: gating scenarios QUARANTINED (not exercised, not passing): $QGATING"
  if [[ "$STRICT_FLAG" == 1 ]]; then echo "!!!! NETNS_STRICT=1: failing the run"; overall=1; fi
fi
exit "$overall"
