# Deployment Guide

Last updated: 2026-10-15. Short entry point. The full runbook is [deploy/README.md](../deploy/README.md); this page lists what runs where and the rules that are easy to miss.

## Hosts

| Host | Mode | Public name | Release | Status |
|------|------|-------------|---------|--------|
| Ohio `cms-node` (18.223.244.168) | Full: backend, relay, Postgres 17 | `api-fsync.10z.one`, `relay-fsync.10z.one` | `519ec821e90f` | Migrations 0001-0003 applied. Real OTP via Gmail SMTP verified 2026-10-15 (204 in 1.8 s). |
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
| `deploy/scripts/` | `deploy.sh`, `smoke-check.sh`, `backup-postgres.sh`, `restore-postgres.sh`, `gen-env.sh`, `bootstrap-vps.sh` |
| `tests/e2e/compose-stack-sync.sh` | Local run of the whole stack with Docker |

## Common Commands

Replace `<ssh-target>` with the host you deploy to. Keep env files out of git.

```bash
# Full stack (Ohio)
deploy/scripts/gen-env.sh api-fsync.10z.one relay-fsync.10z.one > fsync.env
deploy/scripts/deploy.sh <ssh-target> --env-file fsync.env
deploy/scripts/deploy.sh <ssh-target> --rollback

# Relay-only (Singapore)
cp deploy/relay-only.env.example relay-sg.env
deploy/scripts/deploy.sh <ssh-target> --relay-only --env-file relay-sg.env

tests/e2e/compose-stack-sync.sh          # local verification
```

## Rules That Are Easy to Miss

- **DNS-only (grey cloud)** for every public hostname. The relay needs long-lived upgraded connections and UDP, which the Cloudflare proxy does not carry. The backend's per-IP limits need the real client address.
- **The owner applies the host Caddy snippet.** `deploy.sh` never edits or reloads Caddy. Public HTTPS checks (`--public-smoke`) fail until it is in place.
- **Mode guard.** `deploy.sh` records `relay-only` or `full` in `<dir>/.mode`. It refuses `--relay-only` on a full host (it would remove the stack) and refuses a full deploy on a relay-only host.
- **OTP email is Gmail SMTP.** Production refuses to start until `SMTP_USERNAME`, `SMTP_PASSWORD` and `SMTP_FROM` are set (see `deploy/README.md`, "OTP email"). `OTP_DAILY_SEND_CAP` (default 450 per rolling 24 h) keeps the stack under the Gmail limit; use a transactional provider before real traffic. Alert on the backend log line `global OTP send cap reached`: sign-in is refused for everyone until the window frees up.
- **Migrations are forward-only.** Rollback restores the previous images and `compose.yml`, not the database. Keep migrations backward compatible.
- **Backups.** `backup-postgres.sh` runs daily when installed with `deploy.sh --install-backup-cron` (full stack only). Copy dumps off the host and enable provider snapshots; both live on the same disk otherwise.
- **Rotating `JWT_SIGNING_KEY`** signs every user out.

## Related

- [System architecture: deployment topology](./system-architecture.md#deployment-topology)
- [Development roadmap](./development-roadmap.md): phase 10 status
