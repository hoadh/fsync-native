# Deployment Guide

Last updated: 2026-10-14. Short entry point. The full runbook is [deploy/README.md](../deploy/README.md); this page lists what runs where and the rules that are easy to miss.

## What Runs Where

| Component | Listens on | Public name |
|-----------|-----------|-------------|
| Host Caddy (TLS, ACME) | VPS 443 and 80 | `api-fsync.10z.one`, `relay-fsync.10z.one` |
| Backend (`fsync-backend`) | `127.0.0.1:18080` | `api-fsync.10z.one` |
| Relay (`iroh-relay`) | `127.0.0.1:18081` | `relay-fsync.10z.one` |
| Relay QUIC address discovery | `udp/7842`, off by default (`RELAY_QAD=1`) | not used by the proxy |
| Postgres 17 | compose network only | none |

Clip and file bytes never reach the backend or Postgres. The relay sees ciphertext only.

## Files

| Path | Purpose |
|------|---------|
| `deploy/compose.yml` | Production stack (backend, relay, Postgres) |
| `deploy/Caddyfile` | Site blocks for the host Caddy (reference copy) |
| `deploy/.env.example` | All settings and their defaults; no secrets |
| `deploy/scripts/` | `deploy.sh`, `smoke-check.sh`, `backup-postgres.sh`, `restore-postgres.sh`, `gen-env.sh`, `bootstrap-vps.sh` |
| `tests/e2e/compose-stack-sync.sh` | Local run of the whole stack with Docker |

## Common Commands

Replace `<ssh-target>` with the host you deploy to. Keep the generated env file out of git.

```bash
deploy/scripts/gen-env.sh api-fsync.10z.one relay-fsync.10z.one > fsync.env
deploy/scripts/deploy.sh <ssh-target> --env-file fsync.env
deploy/scripts/deploy.sh <ssh-target> --rollback
tests/e2e/compose-stack-sync.sh          # local verification
```

## Rules That Are Easy to Miss

- **DNS-only (grey cloud)** for both hostnames. The relay needs long-lived upgraded connections and UDP, which the Cloudflare proxy does not carry. The backend's per-IP limits need the real client address.
- **The owner applies the host Caddy snippet.** `deploy.sh` never edits or reloads Caddy. Public HTTPS checks (`--public-smoke`) fail until it is in place.
- **OTP email is Gmail SMTP.** Production refuses to start until `SMTP_USERNAME`, `SMTP_PASSWORD` and `SMTP_FROM` are set (see `deploy/README.md`, "OTP email"). `OTP_DAILY_SEND_CAP` (default 450 per rolling 24 h) keeps the stack under the Gmail limit; use a transactional provider before real traffic. Alert on the backend log line `global OTP send cap reached`: sign-in is refused for everyone until the window frees up.
- **Migrations are forward-only.** Rollback restores the previous images and `compose.yml`, not the database. Keep migrations backward compatible.
- **Backups.** `backup-postgres.sh` runs daily when installed with `deploy.sh --install-backup-cron`. Copy dumps off the host and enable provider snapshots; both live on the same disk otherwise.
- **Rotating `JWT_SIGNING_KEY`** signs every user out.

## Related

- [System architecture: deployment topology](./system-architecture.md#deployment-topology)
- [Development roadmap](./development-roadmap.md): phase 10 status
