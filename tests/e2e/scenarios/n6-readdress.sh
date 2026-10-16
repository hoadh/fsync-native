#!/usr/bin/env bash
# N6 (best-effort, non-gating): the sender's address changes mid-transfer. The fetch must still
# complete with a matching hash. A big file keeps the transfer running long enough to interrupt.
scenario_n6() {
  use_pair lan
  local big="${N6_MIB:-1024}"
  # The address change must not leak into later scenarios: the runner's between-scenario reset
  # (hosts_reset) restores it even when this subshell is killed; the trap does it as soon as possible.
  trap hosts_reset EXIT
  ( file_round A B n6 "$big" ) &
  local bg=$!; track "$bg"
  local base; base="$(if_bytes "$A_NS" eth0)"
  local started=0
  for _ in $(seq 1 300); do
    if [[ $(( $(if_bytes "$A_NS" eth0) - base )) -ge 33554432 ]]; then started=1; break; fi
    sleep 0.2
  done
  [[ $started == 1 ]] || { kill "$bg" 2>/dev/null; fail "transfer never started"; }
  log "[n6] re-addressing A: 192.168.10.10 -> 192.168.10.12"
  # removing the primary address would drop the new one too unless secondaries are promoted
  nsx "$A_NS" sysctl -qw net.ipv4.conf.eth0.promote_secondaries=1
  nsx "$A_NS" ip addr add 192.168.10.12/24 dev eth0
  nsx "$A_NS" ip addr del 192.168.10.10/24 dev eth0
  nsx "$A_NS" ip route replace default via 192.168.10.1 || fail "route after re-address"
  metric_add notes "A re-addressed mid-transfer (${big} MiB)"
  wait "$bg" || fail "transfer did not survive the address change"
}
