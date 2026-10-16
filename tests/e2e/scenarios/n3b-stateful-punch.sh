#!/usr/bin/env bash
# N3b: hole punching through stateful firewalls (no address translation). Each site's router drops
# unsolicited inbound packets; the private LANs are routed across the WAN, so the private addresses
# the peers already advertise are reachable once both sides have sent a packet (a pinhole each way).
# This exercises the real punch path (relay-signalled simultaneous probes, direct path upgrade) without
# needing relay QUIC address discovery, which the local relay lacks (see N3).
scenario_n3b() {
  use_pair nat
  nat_mode r-a stateful
  nat_mode r-b stateful
  CLIP_MAX_S=30 clip_round A B n3b
  path_round n3b direct
}
