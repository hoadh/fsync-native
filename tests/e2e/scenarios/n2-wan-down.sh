#!/usr/bin/env bash
# N2: paired devices, then the LAN router's WAN link goes down (no relay, no backend). Discovery
# must come from mDNS and the CLI must start without the backend. If the CLI itself cannot start
# without the backend, that is a product finding: the scenario is QUARANTINED with the command + error.
scenario_n2() {
  use_pair lan
  wan_down r-lan
  probe_offline_start
  CLIP_MAX_S=10 clip_round A B n2   # mDNS discovery time is recorded; generous gate
  path_round n2 lan
}
