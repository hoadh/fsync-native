#!/usr/bin/env bash
# Security best-effort. N8 already covers "dial from a node that was removed from the trust list".
# A dial from a NodeId that was never paired is not expressible with the CLI: it resolves peers
# only from the local paired list, so there is nothing to point at an unpaired id.
scenario_sec() {
  skip_scn "pure unpaired NodeId dial not expressible via CLI (peers resolve from the paired list); covered by core tests clip_security.rs and net_gate_and_paths.rs; removed-peer dial covered by N8"
}
