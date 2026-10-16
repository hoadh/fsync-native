# Deployment Guide

Last updated: 2026-10-15. Short entry point. The full runbook is [deploy/README.md](../deploy/README.md); this page lists what runs where and the rules that are easy to miss.

## Hosts

| Host | Mode | Public name | Release | Status |
|------|------|-------------|---------|--------|
| Ohio `cms-node` (18.223.244.168) | Full: backend, relay, Postgres 17 | `api-fsync.10z.one`, `relay-fsync.10z.one` | `f00980db3978` (main, deployed 2026-10-15; rollback target `549cc6a3d298`) | Migrations 0001-0003 applied. Real OTP via Gmail SMTP verified 2026-10-15 (204 in 1.8 s). Public smoke and `probe-health.sh` all ok after deploy. |
| Singapore Lightsail (52.77.206.192) | Relay only (`--relay-only`) | `relay-sg-fsync.10z.one` | `519ec821e90f` | Docker 29.9 and compose v5.6 installed. `.mode` = `relay-only`. No backend, no Postgres. |

Both hosts are shared with other sites. Their host Caddy files belong to the owner: deploys never edit or reload Caddy.

## What Runs Where

| Component | Listens on | Public name |
|-----------|-----------|-------------|
| Host Caddy (TLS, ACME) | VPS 443 and 80 | per host, see table above |
| Backend (`fsync-backend`) | `127.0.0.1:18080` (Ohio only) | `api-fsync.10z.one` |
| Relay (`iroh-relay`) | `127.0.0.1:18081` (both hosts) | `relay-fsync.10z.one`, `relay-sg-fsync.10z.one` |
| Relay QUIC address discovery | `udp/7842`, off by default (`RELAY_QAD=1`) | not used by the proxy |
| Postgres 17 | compose network only (Ohio only) | none |

Clip and file bytes never reach the backend or Postgres. The relay sees ciphertext only.

## Files

| Path | Purpose |
|------|---------|
| `deploy/compose.yml` | Full stack (backend, relay, Postgres) |
| `deploy/compose.relay-only.yml` | Relay alone, for the relay-only host |
| `deploy/Caddyfile` | Site blocks for the host Caddy (reference copy) |
| `deploy/.env.example` | Full-stack settings and their defaults; no secrets |
| `deploy/relay-only.env.example` | Relay-only settings; no secrets |
| `deploy/scripts/` | `deploy.sh`, `smoke-check.sh`, `rollback-guard.sh`, `probe-health.sh`, `backup-postgres.sh`, `backup-offsite.sh`, `backup-ingest.sh`, `restore-postgres.sh`, `gen-env.sh`, `bootstrap-vps.sh` |
| `.github/workflows/health-probe.yml` | Hourly external health probe (blocked while Actions is unavailable) |
| `tests/e2e/compose-stack-sync.sh` | Local run of the whole stack with Docker |
| `tests/e2e/run-matrix.sh` | Runs the e2e scenarios over relay configs and prints one table |

## Common Commands

Replace `<ssh-target>` with the host you deploy to. Keep env files out of git.

```bash
# Full stack (Ohio)
deploy/scripts/gen-env.sh api-fsync.10z.one relay-fsync.10z.one > fsync.env
deploy/scripts/deploy.sh <ssh-target> --env-file fsync.env
deploy/scripts/deploy.sh <ssh-target> --rollback          # refuses unsafe targets (see Rollback)
deploy/scripts/deploy.sh <ssh-target> --force-rollback    # only after reading rollback-guard.sh

# Relay-only (Singapore)
cp deploy/relay-only.env.example relay-sg.env
deploy/scripts/deploy.sh <ssh-target> --relay-only --env-file relay-sg.env

deploy/scripts/probe-health.sh           # external probe, same checks as the hourly workflow
tests/e2e/compose-stack-sync.sh          # local verification
tests/e2e/run-matrix.sh --scenarios p0,stack --relays local,us,sg   # e2e matrix (--list for names)
```

## Health Probe

`health-probe.yml` runs `probe-health.sh` hourly (17 past the hour) and on dispatch. Checks: `api-fsync.10z.one/health`,
`/health/otp`, `relay-fsync` and `relay-sg-fsync` `/healthz`, and every certificate with at least 14 days left.
A failed scheduled run is the alert (GitHub notifies the owner of the cron line). Scheduled runs can be delayed.

- `/health/otp`: `200 ok`, or `503 otp cap reached` while `OTP_DAILY_SEND_CAP` sends happened in the last 24 h.
  The probe then fails every hour until the window frees up. A release without the endpoint returns 404: the probe only warns.
- The backend also logs `global OTP send cap reached` on every refused start (`docker compose logs backend | grep "OTP send cap"`).

## Rollback

`deploy.sh --rollback` swaps back to the previous release's images and `compose.yml`. It does not touch data.
Before swapping, `scripts/rollback-guard.sh` refuses any target whose migrations do not include the database's
current max version. Without this, sqlx refuses to start the older backend ("migration 3 was previously applied but is missing")
and the API returns 502 until a roll-forward.

On 2026-10-15 a rehearsal on the Ohio host did exactly that: rolling back to a release that predates migration 0003 took the
public API down for about 5 minutes (about 03:08-03:13, +07). The guard was added after that. `--force-rollback` skips the check and
is only for a target you have verified by hand.

A safe pair was rehearsed: `519ec821e90f` <-> `549cc6a3d298`. Each hop took about 88 s; an external 0.5 s poll saw one failed poll per hop.

## Off-Box Backups and Restore

The app host runs `/etc/cron.d/fsync-backup` at 03:15. It runs `backup-offsite.sh`: `pg_dump`, gpg encryption, then an
sftp upload to the Singapore backup host. That host's upload-only user cannot delete or replace stored backups; `backup-ingest.sh`
moves uploads into a store the upload user cannot enter, with 30-day retention and a minimum of 3 files. The encryption key is
public on the app host. The private key lives only in the owner's password manager (and on the owner's workstation under
`~/.fsync-backup/`, mode 600). Losing it makes the off-box copies unreadable.

Failures are visible only in `backup.log` unless `BACKUP_PING_URL` is set in `.backup/offsite.env` (a dead-man's-switch URL, keep it mode 600).

Restore drill (any machine with Docker; production is never touched). Copy one file from the store, then:

```bash
docker run -d --name restore-db -e POSTGRES_PASSWORD=x -e POSTGRES_DB=restore -v "$PWD:/r" postgres:17
docker exec restore-db sh -c 'export GNUPGHOME=$(mktemp -d); gpg --batch --import /r/<private-key>.asc &&
  gpg --batch -d -o /tmp/r.dump /r/<backup>.dump.gpg && pg_restore -U postgres -d restore --no-owner /tmp/r.dump'
docker exec restore-db psql -U postgres -d restore -c 'select count(*) from accounts'; docker rm -f -v restore-db
```

Full steps and host provisioning: [deploy/operations.md](../deploy/operations.md). Last drill (2026-10-15, Mac): decrypt, `pg_restore` and row counts matched live.

## E2E Matrix

`tests/e2e/run-matrix.sh` runs scenarios `p0` (two CLI devices: pair, clip, file direct and forced relay), `stack`
(the compose stack, once) and `netns` (N1-N9 in network namespaces, privileged Docker, once; see [test results](./test-results/README.md)) over relay configs `local`, `us`, `sg`, or an https URL. Each cell has a time limit. It is not CI: `us` and `sg`
are production relays. Last recorded run (MiB=8, `p0`, `local,sg`): both PASS (local 239 s, sg 51 s, sg about 0.38 MiB/s).

## Rules That Are Easy to Miss

- **DNS-only (grey cloud)** for every public hostname. The relay needs long-lived upgraded connections and UDP, which the Cloudflare proxy does not carry. The backend's per-IP limits need the real client address.
- **The owner applies the host Caddy snippet.** `deploy.sh` never edits or reloads Caddy. Public HTTPS checks (`--public-smoke`) fail until it is in place.
- **Mode guard.** `deploy.sh` records `relay-only` or `full` in `<dir>/.mode`. It refuses `--relay-only` on a full host (it would remove the stack) and refuses a full deploy on a relay-only host.
- **OTP email is Gmail SMTP.** Production refuses to start until `SMTP_USERNAME`, `SMTP_PASSWORD` and `SMTP_FROM` are set (see `deploy/README.md`, "OTP email"). `OTP_DAILY_SEND_CAP` (default 450 per rolling 24 h) keeps the stack under the Gmail limit; use a transactional provider before real traffic. Alert on the backend log line `global OTP send cap reached`: sign-in is refused for everyone until the window frees up.
- **Migrations are forward-only.** Rollback restores the previous images and `compose.yml`, not the database. `rollback-guard.sh` refuses targets that predate the current migration; keep migrations backward compatible anyway.
- **Backups.** Nightly off-box copies run once `.backup/offsite.env` exists (see Off-Box Backups). Local dumps stay on the same disk for 14 days; enable provider snapshots too. A failed night is silent unless `BACKUP_PING_URL` is set.
- **Rotating `JWT_SIGNING_KEY`** signs every user out.

## Client Release Builds

Release clients must carry both relays. A client with no relay is LAN only, and no default is built in.

| Client | Command or setting |
|--------|--------------------|
| macOS | `apps/macos/scripts/build-fsync-mac-app.sh --api https://api-fsync.10z.one --relay https://relay-fsync.10z.one,https://relay-sg-fsync.10z.one` (add `--sign`, `--profile` for Developer ID). Writes Info.plist `FsyncRelayUrl`. |
| Windows | `FSYNC_RELAY=https://relay-fsync.10z.one,https://relay-sg-fsync.10z.one` in the environment the app starts from. Read at launch; no build-time value. |

Rules:
- The list is comma-separated and https only. One bad entry fails engine start for the whole list. Order does not matter: the home relay is picked by latency.
- The macOS build needs `apps/macos/Packages/FsyncCoreKit/FsyncFFI.xcframework`: run `scripts/build-xcframework.sh` first.
- Rebuild `fsync_ffi.dll` whenever the Rust FFI or its bindings change: `scripts/gen-bindings-cs.ps1` (C#), then `scripts/build-windows-dll.ps1`, which copies the DLL into `apps/windows/src/Fsync.CoreBindings/runtimes/<rid>/native`. Ship the DLL and the generated `fsync_ffi.cs` together. A mismatch fails at record decode.
- Same rule on macOS: rerun `scripts/build-xcframework.sh` after an FFI change.

## Related

- [System architecture: deployment topology](./system-architecture.md#deployment-topology)
- [Development roadmap](./development-roadmap.md): phase 10 status
