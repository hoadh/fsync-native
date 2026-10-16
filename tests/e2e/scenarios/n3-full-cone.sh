#!/usr/bin/env bash
# N3: A and B behind separate full-cone NATs (endpoint-independent mapping + filtering), shared relay.
# Expected: hole punch -> direct path (no relay bytes, WAN bytes on A's router).
#
# Known limit: the local relay has no QUIC address discovery (the client trusts webpki roots only, so
# a relay with a private CA cannot be used for QAD). Peers then never learn their public mappings and
# only offer private candidates, so the punch cannot start. While that holds, this scenario is
# QUARANTINED with the observed counters; it turns PASS by itself once a direct path appears.
# N3b covers the punch machinery without needing QAD.
scenario_n3() {
  use_pair nat
  nat_mode r-a full-cone
  nat_mode r-b full-cone
  CLIP_MAX_S=30 clip_round A B n3   # latency recorded, not judged here (see N4)
  path_round n3 -
  case "$PATH_SEEN" in
    direct) return 0;;
    relay) quarantine_scn "no direct path behind full-cone NATs: local relay has no QUIC address discovery (webpki-only trust), peers only know private candidates; file went via relay (${PATH_RELAY_BYTES} B on relay veth)";;
    *) fail "[n3] unexpected path '$PATH_SEEN'";;
  esac
}
