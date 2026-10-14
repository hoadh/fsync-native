# Code review: infra/phase-10, round 3 (HEAD 66dfcd3)

Score 9.6/10. 0 critical, 0 high, 0 medium, 5 low. Gate met.

## Round-2 items: all fixed
- N1: per-deploy `mktemp -d` staging, EXIT trap cleans it, flock still serializes the switch.
- N2: CI filters cover gen-bindings/build scripts and core; main push + workflow_dispatch run all jobs. actionlint clean.
- N8: compose.yml/Caddyfile 644, scripts 755, .env 600; README says the snippet is pasted manually into /etc/caddy/Caddyfile.
- N5: max_connections 30 (host confirmed). N6: `._*` removed, none on host. N4: OOM/restart warning in smoke-check.
- Host (read-only): 3 containers healthy, release d9e5e9b live.

## Low
- L1 smoke-check.sh:65-69 OOM inspect pipeline can abort under set -e; add `|| true`.
- L2 "oom/restart check" ok line printed after warnings (cosmetic).
- L3 compose.yml.prev still 600 (fixes itself on next rotation).
- L4 CI never run on GitHub; path filters verified by reading + actionlint only.
- L5 compose-stack-sync e2e duplicates p0-exit-demo setup (known debt).

## Unresolved
- Are mac/win CI jobs required checks (skip counts as pass)?
