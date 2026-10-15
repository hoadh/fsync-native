# Phase 10 — Infra: self-hosted iroh-relay, backend deploy, CI/CD

## Context Links
- Design §2 Networking, §5 relay cost; ADR 007 ([phase-02](phase-02-spike-iroh-discovery-relay.md)); [phase-09](phase-09-backend-auth-registry-presence.md)

## Overview
- Priority: P0 | Status: Partial (shared VPS staging live 2026-10-14; SG relay-only live 2026-10-15 (S2/S6); rollback rehearsed and off-box backups live 2026-10-15; alerts implemented, unverified on GitHub; prod split and backend-deploy workflow open) | Effort: 1-1.5 wk (B)
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
- [~] DNS done (`api-fsync` grey cloud, `relay-fsync` .10z.one, public TLS OK); mail auth records (SPF/DKIM/DMARC) pending, no provider yet
- [x] Relay stack (iroh-relay 1.3.0 image, limits, optional QAD) - deployed, public TLS smoke ok (relay-fsync.10z.one, Ohio)
- [x] Second region: Singapore relay-only at relay-sg-fsync.10z.one (S2/S6, 2026-10-15). Both relays in app configs; e2e PASS
- [x] Backend + Postgres stack, deploy/rollback/smoke scripts, `pg_dump` backup (cron 03:15 on cms-node + restore script); off-box copy done 2026-10-15 (D4: nightly gpg-encrypted to SG, 30 d retention, restore drilled; failure visibility log-only until owner sets `BACKUP_PING_URL`)
- [~] CI: `.github/workflows/ci.yml` (rust, macOS swift, Windows dotnet) + `deploy-stack.yml`. Each job green on some GitHub run (rust 38630595475/38635166526; macOS 38638772422; Windows 38636795483), none on one run of the final main. New runs blocked by GitHub billing (owner).
- [x] Public two-client e2e (staging, from Mac): login, pair (SAS), clip direct 22 ms / relay 802 ms, 16 MiB file direct and relay hash-verified
- [ ] Monitoring + alerts (D2 implemented on infra/round-2: `deploy/scripts/probe-health.sh`, hourly `health-probe.yml`, `GET /health/otp` 503 on cap; verified only after push + GitHub run)
- [ ] Core artifacts workflow
- [~] Deployment runbook (in `deploy/README.md` and `deploy/operations.md`; `docs/deployment-guide.md` not written)
- [ ] Staging/prod split, backend-deploy workflow (rollback rehearsal done 2026-10-15, see Round 2)

## Owner Action Needed
1-3. Done 2026-10-14: Caddy site blocks applied; DNS for both hosts (api-fsync must stay grey cloud, else per-IP limits bucket by edge); `deploy.sh --public-smoke` ok.
4. Lightsail firewall: 443/80/22 already open; open 7842/udp only if enabling QAD (`RELAY_QAD=1`).
5. Email: Gmail SMTP sender done 2026-10-15 (S1, deployed). Mail auth checked 2026-10-15 (D5, read-only): SPF ok; DMARC `p=none`; DKIM not found at common selectors. Owner: enable DKIM in Google Admin, publish `google._domainkey` TXT, then tighten DMARC. Still open: confirm `FSYNC_ENV=production` and dev mode off on cms-node (unverified).
6. Off-box dumps done 2026-10-15 (D4). Owner: copy backup private key `~/.fsync-backup/fsync-backup-secret.asc` (Mac) to password manager; choose `BACKUP_PING_URL` or accept log-only. Still open: Lightsail snapshots; second region decision is closed (SG + US/EU, see phase 10 Overview).
7. RAM is tight (~600 MiB available on the shared host); consider a dedicated VPS before launch.

## Success Criteria
- CLI pairs + transfers via prod relay with `--force-relay`; backend deploy rollback rehearsed once in staging; alerts fire on simulated relay outage.
- 2026-10-14 status: pairs + transfers via public staging relay PASS.
- 2026-10-15 status: two-relay e2e PASS (SG relay-only clip 0.42 s, 8 MiB 0.63 MiB/s; both relays clip 1.18 s, 8 MiB via relay 0.55 MiB/s). Round 2: rollback rehearsed (D3), off-box backups restore-tested (D4), `run-matrix.sh` p0 PASS local + SG (D7). Open: alert verification after push, alert firing test on simulated relay outage, prod split, backend-deploy workflow, one green GitHub CI run on final main.

## Round 2 (Track D, 2026-10-15, branch infra/round-2, merged local main 3c405b1)
- D1: rust job failure was `ld` SIGBUS (disk/debuginfo) linking core test binaries; fixed (debuginfo off + free-disk step, 549cc6a). macOS and Windows job failures seen in round 2 were app code, fixed by Track C/B; not yet re-run on GitHub.
- D2: `GET /health/otp` 200 / 503 "otp cap reached" (test `auth_otp_budget.rs`). Probe script checks api, both relays, cert expiry >= 14 d; hourly workflow. OTP-cap alert repeats hourly until cleared (intentional). Failure = red run = GitHub notification.
- D3: rollback rehearsal on cms-node. First attempt (to `d9e5e9b7ef77`, no migration 0003) crash-looped the backend and caused ~5 min api 502 (03:08-03:13 local) until rolled forward. Fixed: `deploy/scripts/rollback-guard.sh` refuses targets whose migrations lack the DB max version; `--force-rollback` to override. Valid rehearsal (`549cc6a3d298` → `519ec821e90f` → forward): ~88 s per hop, one failed poll per hop, smoke ok. cms-node now runs `549cc6a3d298` (branch build); redeploy from main pending.
- D4: nightly 03:15 `pg_dump` → gpg (public key on cms-node only) → sftp to SG upload-only → SG ingest every 10 min, 30 d retention (min 3), local 14 d. Restore drill on Mac in throwaway postgres:17: decrypt, `pg_restore`, counts match.
- D5: see Owner Action 5. D6: smtp_mailpit flake fixed (`wait_ready`, polling, `no_mail_to`; 8/8 + full workspace). D7: `tests/e2e/run-matrix.sh` (scenarios p0/stack; relays local/us/sg/URL; watchdog; `--list`); phase 15 harness skeleton only.

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
