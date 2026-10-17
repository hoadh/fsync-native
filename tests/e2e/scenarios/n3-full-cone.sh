#!/usr/bin/env bash
# N3: A and B behind separate full-cone NATs (endpoint-independent mapping + filtering), shared relay.
# Expected: hole punch -> direct path (no relay bytes, WAN bytes on A's router).
#
# Needs the relay's QUIC address discovery so peers learn their NAT-mapped public address: the local
# relay runs QAD and the CLI trusts its private CA (relay_certs in lib/netns-services.sh, fsync-cli
# built with the test-relay-ca feature). Gating: the file must take the direct path.
scenario_n3() {
  use_pair nat
  nat_mode r-a full-cone
  nat_mode r-b full-cone
  CLIP_MAX_S=30 clip_round A B n3   # latency recorded, not judged here (see N4)
  path_round n3 direct
}
