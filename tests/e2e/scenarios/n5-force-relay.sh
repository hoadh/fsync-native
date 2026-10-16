#!/usr/bin/env bash
# N5: same LAN as N1 but both sides run with --force-relay: every byte must cross the relay.
# Throughput is recorded (MiB/s), not gated.
scenario_n5() {
  use_pair lan
  # shellcheck disable=SC2034  # read by cli() in cli-session.sh
  EXTRA=(--force-relay)
  CLIP_MAX_S=5 clip_round A B n5
  path_round n5 relay
}
