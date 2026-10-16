#!/usr/bin/env bash
# shellcheck disable=SC2034  # globals are consumed by the sourcing scripts
# Backend + relay processes and per-scenario path measurement for the netns matrix. Sourced in the
# runner container after netns-topology.sh and cli-session.sh. Needs WORK, BIN_DIR, DATABASE_URL.

API_ADDR=100.64.0.1:8080
RELAY_PORT=3340

backend_start() {
  DATABASE_URL="$DATABASE_URL" BIND_ADDR="$API_ADDR" FSYNC_DEV_MODE=1 \
    RUST_LOG="fsync_backend=debug,info" NO_COLOR=1 "$BIN_DIR/fsync-backend" >>"$WORK/backend.log" 2>&1 &
  echo $! >"$WORK/backend.pid"
  local i
  for i in $(seq 1 100); do curl -fs "http://$API_ADDR/health" >/dev/null 2>&1 && return 0; sleep 0.2; done
  fail "backend not healthy"
}

backend_stop() {
  [[ -f "$WORK/backend.pid" ]] || return 0
  kill "$(cat "$WORK/backend.pid")" 2>/dev/null || true
  local i
  for i in $(seq 1 50); do curl -fs --max-time 1 "http://$API_ADDR/health" >/dev/null 2>&1 || return 0; sleep 0.2; done
  fail "backend did not stop"
}

# cli_kill [NAME]: stop the CLI process(es) of one device (a backgrounded `cli` is a subshell, so
# its pid is not the CLI's), or all of them.
cli_kill() {
  if [[ -n "${1:-}" ]]; then pkill -f -- "fsync-cli --data $WORK/$1 " 2>/dev/null || true
  else pkill -f -- "$BIN_DIR/fsync-cli " 2>/dev/null || true; fi
}

backend_up() { curl -fs --max-time 2 "http://$API_ADDR/health" >/dev/null 2>&1; }

# The spike relay binds 127.0.0.1 in its own ns; DNAT exposes it on the WAN address (route_localnet).
relay_start() {
  nsx relay "$BIN_DIR/spike/iroh-discovery-spike" relay "$RELAY_PORT" >"$WORK/relay.log" 2>&1 &
  echo $! >"$WORK/relay.pid"
  wait_for '^RELAY http' "$WORK/relay.log" 20
  nsx relay iptables -t nat -A PREROUTING -d 100.64.0.2 -p tcp --dport "$RELAY_PORT" \
    -j DNAT --to-destination "127.0.0.1:$RELAY_PORT"
  curl -fs --max-time 3 -o /dev/null "http://100.64.0.2:$RELAY_PORT/" || log "warn: relay root not answering on WAN"
}

# probe_offline_start: a plain CLI start must work with the backend unreachable (N2, N9). If it does
# not, that is a product finding: quarantine with the exact command and error.
probe_offline_start() {
  if ! cli A devices --json >"$WORK/probe.out" 2>"$WORK/probe.err"; then
    quarantine_scn "CLI cannot start with backend unreachable. cmd: fsync-cli --data A --api $API --relay $RELAY --insecure-http devices --json ; error: $(head -c 300 "$WORK/probe.err" | tr '\n\t' '  ')"
  fi
}

# use_pair lan|nat: which namespaces devices A and B run in, and which routers count as their WAN.
use_pair() {
  case "$1" in
    lan) A_NS=h-la; B_NS=h-lb; A_RTR=r-lan; B_RTR=r-lan;;
    nat) A_NS=h-na; B_NS=h-nb; A_RTR=r-a; B_RTR=r-b;;
    *) fail "use_pair: $1";;
  esac
  CLI_NS_A="$A_NS"; CLI_NS_B="$B_NS"
}

# path_round LABEL EXPECT [MIB]: A -> B file round, counter deltas over it, classified path.
# Fails when the observed path is not EXPECT ("-": record only; result in PATH_SEEN). Records bytes + path in the scenario metrics.
path_round() {
  local label="$1" expect="$2" mib="${3:-$MIB}" size b a
  size=$((mib * 1048576))
  counters_snap "$WORK/snap.before"
  file_round A B "$label" "$mib"
  counters_snap "$WORK/snap.after"
  read -r -a b <"$WORK/snap.before"; read -r -a a <"$WORK/snap.after"
  local relay=$((a[0] - b[0])) wan_a=$((a[1] - b[1])) wan_b=$((a[2] - b[2])) host_a=$((a[3] - b[3]))
  local seen; seen="$(classify_path "$size" "$relay" "$wan_a" "$host_a")"
  log "[$label] bytes relay=$relay wan_a=$wan_a wan_b=$wan_b host_a=$host_a -> path=$seen (expect $expect)"
  metric_add path_expected "$expect"; metric_add path_observed "$seen"
  metric_add bytes_relay "$relay"; metric_add bytes_wan_a "$wan_a"
  metric_add bytes_wan_b "$wan_b"; metric_add bytes_host_a "$host_a"
  # Informational only (the byte counters above are authoritative). Absent on CLIs that do not emit
  # `path` on sent / `path_tier` on fetched: the JSON keeps null then.
  local cp ct
  cp="$(jfield "$WORK/round-$label/send.out" sent path)"; ct="$(jfield "$WORK/round-$label/fetch.out" fetched path_tier)"
  if [[ -n "$cp" ]]; then metric_add cli_path "$cp"; fi
  if [[ -n "$ct" ]]; then metric_add cli_path_tier "$ct"; fi
  PATH_SEEN="$seen"; PATH_RELAY_BYTES="$relay"
  [[ "$expect" == "-" ]] || assert_eq "$seen" "$expect" "[$label] observed path"
}
