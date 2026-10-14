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

## Review fixes round (code-reviewer 7.5/10), 2026-10-14
- H1 bootstrap-vps.sh: requires `--dedicated`; aborts on non-fsync containers or a listener on 80/443.
- H2 compose network pinned (`FSYNC_SUBNET`, default 172.29.0.0/24, free on cms-node: docker nets 172.17-19, VPC 172.26.0.0/20); TRUSTED_PROXIES = subnet + 127.0.0.0/8. New `CHECK_PROXY_TRUST=1` smoke step (14 unauth presence sockets with distinct X-Forwarded-For; passes only if the peer is trusted). Runs in deploy.sh on-host and in the e2e. Passed on cms-node via docker-proxy, which is the same loopback path host Caddy uses. Not testable here: real Caddy XFF (snippet not applied).
- Cloudflare: README/.env.example/phase file now say api-fsync DNS-only; Cloudflare-ranges + hops=2 only documented as alternative.
- M1 QAD mounts only the relay host cert dir (`RELAY_CERT_DIR`, replaces RELAY_CERT_VOLUME). M2 relay is uid 10001 unless `RELAY_USER=0` (QAD only; verified `user=10001:10001` on cms-node).
- M3 `deploy.sh --install-backup-cron` (writes only /etc/cron.d/fsync-backup; installed on cms-node, daily 03:15), warning when not scheduled, free-disk check (1 GiB), umask 077, `restore-postgres.sh` (user/db from container env).
- M4 remote flock, atomic .env (temp + mv, grep re-terminates last line). M5 `compose.yml.prev` kept and swapped on `--rollback`; rollback rehearsed on cms-node and rolled forward again; migrations documented forward-only.
- M6 ci.yml `changes` job gates macos/windows by path (dispatch = all); deploy-stack.yml uses buildx gha layer cache + `SKIP_BUILD=1`. Third-party actions pinned by SHA (rust-toolchain needs explicit `toolchain: 1.99.0`). Not run on GitHub yet; actionlint clean.
- M7 postgres max_connections=25 (backend pool 20, db.rs), memswap_limit = mem_limit for all three.
- Lows: root `.dockerignore` (note: deploy/backend/Dockerfile.dockerignore already excluded target for the backend build), COPYFILE_DISABLE=1, WS probe `-m 3`, `--remote-build` removed.
- Redeploy: release 89df2da43875, subnet 172.29.0.0/24, ~27 MiB total. Remote .env had its old TRUSTED_PROXIES line removed (was 172.16.0.0/12).
- Local e2e re-run PASS (random /24, restore script, proxy-trust check).
- Open: positive proxy-trust check only (negative case not exercised); QAD path with the host-dir mount untested on the VPS.

## Review round 2 fixes (8.8/10)
- N1 staging in a unique `mktemp -d /opt/fsync/.incoming.XXXXXX`, removed on exit; lock still guards the switch.
- N2 ci.yml filters add gen-bindings.sh / gen-bindings-cs.ps1; push to main and dispatch always run all jobs.
- N8 compose.yml, Caddyfile, scripts 644/755 (.env stays 600); README states host Caddy does not import /opt/fsync/Caddyfile, owner pastes into /etc/caddy/Caddyfile.
- N5 max_connections=30 (verified on host). N6 stale `._*` files deleted on cms-node (verified), not recreated.
- N4 smoke-check.sh warns on OOMKilled/restarts (`OOM_CHECK_DOCKER`, `FSYNC_PROJECT`); never fails.
- Redeployed d9e5e9b7ef77; e2e PASS.
