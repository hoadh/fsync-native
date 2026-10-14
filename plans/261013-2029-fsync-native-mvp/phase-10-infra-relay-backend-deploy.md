# Phase 10 — Infra: self-hosted iroh-relay, backend deploy, CI/CD

## Context Links
- Design §2 Networking, §5 relay cost; ADR 007 ([phase-02](phase-02-spike-iroh-discovery-relay.md)); [phase-09](phase-09-backend-auth-registry-presence.md)

## Overview
- Priority: P0 | Status: partial (single-VPS staging deployed on loopback; public HTTPS pending owner actions) | Effort: 1-1.5 wk (B)
- Deploy staging + prod: backend (container) + managed Postgres + iroh-relay in **2 regions at launch (Singapore + US/EU)** <!-- Updated: Validation Session 1 - 2 relay regions -->, TLS, monitoring, CI pipelines producing core artifacts.

## Key Insights
- iroh-relay needs public IP, TLS (ACME), UDP for QUIC address discovery; bandwidth is main cost → per-connection rate limit config.
- Relay sees only encrypted QUIC packets; still treat as sensitive infra (metadata: NodeIds, IPs).
- Keep infra minimal: 1 VM per relay region, backend on small container platform/VM, managed Postgres with PITR.

## Requirements
- Envs: `staging`, `prod` (separate DB, relay, signing of JWT keys).
- Observability: relay bandwidth + connection count, backend RED metrics, WS count; alerts on relay down, 5xx > 1%.
- Backups: Postgres daily + PITR 7 d.

## Architecture
```
deploy/
  relay/   iroh-relay.toml (hostname, ACME, rate limits), relay.service | compose.yml, README.md
  backend/ Dockerfile (multi-stage, distroless), compose.yml (staging), deploy.sh, .env.example
  monitoring/ prometheus scrape + alert rules (or hosted equivalent)
.github/workflows/
  ci.yml            # fmt, clippy, test (3 OS), backend integration (Postgres service)
  core-artifacts.yml# xcframework (macos runner), DLL x64/ARM64 (windows runner), uploads
  backend-deploy.yml# build image → staging auto, prod manual approval
```
Client config: `relay_urls = [https://relay-sg.<domain>, https://relay-us.<domain>]` (iroh RelayMap; client picks lowest latency as home relay), `api_base = https://api.<domain>` baked per build channel.

## Related Code Files
- Create: files above
- Modify: `.github/workflows/ci.yml` (from phase 01)

## Implementation Steps
1. Domains/DNS, SPF/DKIM/DMARC for OTP mail.
2. Relay VMs (SG + US/EU) + config + ACME; verify with CLI `--force-relay`, incl. peers homed on different regions.
3. Managed Postgres + backend container; staging deploy pipeline; prod w/ approval.
4. Metrics + alerts + uptime checks.
5. Core artifacts workflow consumed by app CI (11/13).
6. Runbook `docs/deployment-guide.md` (deploy, rollback, rotate keys).

## Implementation Status (Track D, 2026-10-14)
Single shared Lightsail VPS (`ubuntu@cms-node`, 18.223.244.168) instead of managed Postgres + 2 regions: stack in `/opt/fsync`,
compose project `fsync`, prebuilt linux/amd64 images tagged by git sha, backend `127.0.0.1:18080`, relay `127.0.0.1:18081`,
env `staging` + dev mode (OTP in logs; no email provider yet). Host Caddy untouched. Details: `deploy/README.md`,
`plans/reports/infra-track-261014-1510-phase-10.md`. Local compose e2e (`tests/e2e/compose-stack-sync.sh`) passes.

## Todo List
- [ ] DNS + mail auth records (DNS: see owner actions; mail: no provider yet)
- [x] Relay stack (iroh-relay 1.3.0 image, limits, optional QAD) - deployed, loopback smoke ok; public/prod region pair pending
- [x] Backend + Postgres stack, deploy/rollback/smoke scripts, `pg_dump` backup script (manual run verified; cron not installed)
- [x] CI: `.github/workflows/ci.yml` (rust, macOS swift, Windows dotnet; Windows job unverified on a real runner) + `deploy-stack.yml`
- [ ] Monitoring + alerts
- [ ] Core artifacts workflow
- [~] Deployment runbook (in `deploy/README.md`; `docs/deployment-guide.md` not written)
- [ ] Second relay region, staging/prod split, backend-deploy workflow, rollback rehearsal on the VPS

## Owner Action Needed
1. Append the two site blocks from `deploy/README.md` to `/etc/caddy/Caddyfile`, `caddy validate`, reload Caddy.
2. DNS: `relay-fsync.10z.one` DNS-only (grey cloud); `api-fsync.10z.one` may stay proxied. Both -> 18.223.244.168.
3. Then run `deploy/scripts/deploy.sh ubuntu@cms-node --public-smoke`.
4. Lightsail firewall: 443/80/22 already open; open 7842/udp only if enabling QAD (`RELAY_QAD=1`).
5. Email provider + secret (and a backend sender; none exists) before any real user; then switch `FSYNC_ENV=production`, dev mode off.
6. Enable Lightsail snapshots; add backup cron and off-box copy; decide second region (SG + US/EU).
7. RAM is tight (~600 MiB available on the shared host); consider a dedicated VPS before launch.

## Success Criteria
- CLI pairs + transfers via prod relay with `--force-relay`; backend deploy rollback rehearsed once in staging; alerts fire on simulated relay outage.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Relay bandwidth cost spike / abuse | M x M | restrict relay to authenticated clients if iroh-relay 1.x access control allows, else rate limits + monitoring; per-conn caps |
| Relay latency for users far from both regions (e.g. EU if US chosen) | M x M | measure in phase 15; add 3rd region if p95 > 1.5 s |
| Cross-region: peers on different home relays | M x L | iroh dials peer's home relay; verify in phase 15 matrix |
| Secrets leak in CI | L x H | GitHub environments, OIDC to cloud, no long-lived keys |

## Rollback
- Backend: redeploy previous image tag (immutable tags); DB migrations backward-compatible (expand/contract). Relay: previous binary/config kept; DNS TTL 60 s for failover.

## Security Considerations
- Firewall: relay 443/tcp + QUIC ports; backend only 443; DB private network; SSH via keys/SSO only; relay access restricted to authenticated clients if supported (check iroh-relay access config) — otherwise open relay acceptable (E2E) with rate limits.

## Next Steps
- Phase 15 uses staging; phase 16 uses prod.
