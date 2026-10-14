# Infra track (phase 10) report, 2026-10-14

Branch `infra/phase-10` (pushed). Status: partial, deployed to the VPS on loopback; public HTTPS smoke PENDING owner actions.

## Deployed
- Host `ubuntu@cms-node` (Ubuntu 24.04 x86_64), dir `/opt/fsync`, compose project `fsync` (network `fsync_default`, volumes `fsync_pgdata`, `fsync_relay_nocert`).
- Images `fsync-backend`/`fsync-relay` tagged by git sha (cross-compiled locally, shipped with docker save|load); `postgres:17`.
- Backend 127.0.0.1:18080, relay 127.0.0.1:18081 (relay UDP 7842 bound to loopback only). All healthy; loopback smoke ok; idempotent re-deploy verified; pg backup run once (`/opt/fsync/backups`).
- Memory: postgres ~25 MiB, relay ~4 MiB, backend ~1 MiB at idle (limits 192/96/96 MiB).
- `/opt/fsync/.env` mode 600, secrets generated locally, not in git. Env is `staging` + `FSYNC_DEV_MODE=1` (OTP via `docker compose logs backend | grep "dev otp"`), because no email sender exists. Never for real users.
- Not touched: Caddy, mysql/moodle/wordpress/redis, firewall, anything outside /opt/fsync.

## Caddyfile snippet (owner)
```
api-fsync.10z.one {
	reverse_proxy 127.0.0.1:18080
}

relay-fsync.10z.one {
	reverse_proxy 127.0.0.1:18081 {
		flush_interval -1
	}
}
```
Then `sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy`, then `deploy/scripts/deploy.sh ubuntu@cms-node --public-smoke`.

## Lightsail ports / DNS
- 443/tcp, 80/tcp, 22/tcp already open. 7842/udp only if enabling QAD (RELAY_QAD=1, RELAY_QUIC_BIND=0.0.0.0, RELAY_CERT_VOLUME=Caddy data dir).
- DNS: relay-fsync.10z.one must be DNS-only (grey); api-fsync may stay proxied. Both to 18.223.244.168.

## Roll back / stop
- `deploy/scripts/deploy.sh ubuntu@cms-node --rollback` (previous tag).
- Stop: `ssh ubuntu@cms-node 'cd /opt/fsync && sudo docker compose -p fsync down'`; `down -v` deletes data.

## Verification
- Local: `tests/e2e/compose-stack-sync.sh` PASS (compose up, health, HTTPS via local Caddy, 2 CLIs login+pair, clip direct, clip + 4 MiB file via relay, pg backup+restore, `down -v`). `mystifying_euclid` untouched.
- actionlint clean on both workflows; shellcheck -S warning clean on scripts.
- CI workflows not yet run on GitHub (triggers: PR, push main, dispatch).

## Doc content for lead (shared docs not edited)
- `deploy/` now holds the production stack (compose.yml, compose.local.yml, Caddyfile, relay/, backend/Dockerfile, scripts/, README). Older `deploy/backend/compose.yml` is dev-only. codebase-summary/system-architecture/README statements "deploy/relay is empty" or pointing to `deploy/backend/compose.yml` are outdated.
- Changelog entry: production deploy stack, deploy/backup/smoke scripts, CI (rust, macOS, Windows), compose e2e.

## Findings
- Backend has no real email sender: production mode cannot deliver OTPs. No backend change was needed otherwise (`/health`, BIND_ADDR exist).
- QEMU amd64 emulation segfaults rustc, so Dockerfiles cross-compile on the build host (no RUN in runtime stage).
- iroh-relay access control exists (shared token/allowlist/http) but clients lack token support: relay is open (E2E) with accept-rate limits and optional rx cap.
- fsync-cli trusts only webpki roots, so local test uses `--insecure-http` on loopback; real ACME/CA trust untested until Caddy snippet is live.
- TRUSTED_PROXIES=172.16.0.0/12 assumes host Caddy shows up as docker bridge peer; verify after Caddy is live (backend uses TRUST_PROXY_HOPS=1).
- macOS integration script (open question 6) needs Docker for Postgres; GitHub macOS runners have none. Needs an external-Postgres option in `apps/macos/scripts/run-fsync-mac-integration.sh` (apps owner). CI only runs swift tests (3 integration tests skip).
- Windows CI job unverified on a real runner; ARM64 DLL not built in CI.
- compose-stack-sync.sh duplicates p0-exit-demo.sh setup (DRY debt).
- Not done: monitoring/alerts, core-artifacts workflow, backend-deploy workflow, backup cron, second region.

## Unresolved questions
1. Email provider and sender implementation timing?
2. Dedicated VPS for launch (shared host has ~600 MiB free)?
3. Second relay region location (US vs EU) and SG VPS?
4. Enable QAD (needs root-readable Caddy cert dir + udp/7842)?
5. Is the GitHub repo allowed macOS/Windows runner minutes for every PR?
