#!/usr/bin/env bash
# N9: backend process stopped (connection refused), relay still up. LAN clip + file keep working
# from the cached device list. A CLI that cannot start without the backend is quarantined (see N2).
scenario_n9() {
  use_pair lan
  backend_stop
  probe_offline_start
  CLIP_MAX_S=10 clip_round A B n9
  path_round n9 lan
}
