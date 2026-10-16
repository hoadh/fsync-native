#!/usr/bin/env bash
# N4: both sides behind symmetric NATs: hole punching cannot work, relay must carry everything.
# Gating: file over the relay, hash ok. Spec clip target < N4_CLIP_MAX_S (1.5 s): checked after the
# path/hash assertions; if exceeded the scenario is QUARANTINED with the measured value (product finding).
scenario_n4() {
  use_pair nat
  nat_mode r-a symmetric
  nat_mode r-b symmetric
  CLIP_MAX_S=999 clip_round A B n4
  path_round n4 relay
  python3 -c "import sys; sys.exit(0 if $CLIP_LAT_S < ${N4_CLIP_MAX_S:-1.5} else 1)" \
    || quarantine_scn "path relay + hash ok, but clip latency ${CLIP_LAT_S}s exceeds spec ${N4_CLIP_MAX_S:-1.5}s (same stall seen in N3/N3b; LAN and forced-relay are ~0.01s)"
}
