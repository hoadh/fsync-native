#!/usr/bin/env bash
# N1: two devices on one LAN behind a router, WAN up. Clip under CLIP_MAX_S (spec target 0.3 s is
# recorded, not gated), file path must be LAN (no relay bytes, no WAN bytes).
scenario_n1() {
  use_pair lan
  clip_round A B n1
  path_round n1 "${N1_EXPECT_PATH:-lan}"   # override only to prove the assertion bites
}
