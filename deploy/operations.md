# Operations: health probe, off-box backups, rollback

## Health probe and alerts
`.github/workflows/health-probe.yml` runs `scripts/probe-health.sh` hourly (private repo: one run bills a
minute, ~720 of 2000 free minutes a month; edit the cron for faster). It checks `api-fsync.10z.one/health`,
`/health/otp`, `relay-fsync.10z.one/healthz`, `relay-sg-fsync.10z.one/healthz` and that every certificate has
>= 14 days left. A failed scheduled run emails the repo owner (GitHub notification settings: Actions, "Send
notifications for failed workflows only"). Run it by hand: `deploy/scripts/probe-health.sh`.

`/health/otp` is the "global OTP send cap reached" signal: 200 `ok`, or 503 `otp cap reached` while
`OTP_DAILY_SEND_CAP` sends happened in the last 24 h (sign-ins are refused until the oldest slot expires).
The backend also logs `WARN global OTP send cap reached; start refused` on every refused start
(`docker compose logs backend | grep "OTP send cap"`). A release without the endpoint answers 404: the probe warns only.

## Off-box Postgres backups
Cron (`/etc/cron.d/fsync-backup`, 03:15) runs `scripts/backup-offsite.sh`: `pg_dump` into `backups/` (14 days), then
gpg-encrypts the dump and pushes it by sftp to a second host (30 days kept there, pruned by name timestamp).
Config: `<dir>/.backup/offsite.env` (no secrets; keys documented in the script header), ssh key
`.backup/id_ed25519`, pinned `.backup/known_hosts`, gpg home `.backup/gnupg` holding **only the public key**.
The remote account's `authorized_keys` line is `restrict,command="internal-sftp -d <backup dir> -u 077"`: the key
can only use sftp, starting in the backup directory.

The private key is generated once and kept off both hosts (password manager); losing it makes the off-box copies
unreadable. Restore drill (any machine with docker; the production database is never touched):
```bash
scp <backup-host>:fsync-backups/fsync-<ts>.dump.gpg .
docker run -d --name restore-db -e POSTGRES_PASSWORD=x -e POSTGRES_DB=restore -v "$PWD:/r" postgres:17
docker exec restore-db sh -c 'export GNUPGHOME=$(mktemp -d); gpg --batch --import /r/<secret.asc> &&
  gpg --batch -d -o /tmp/r.dump /r/fsync-<ts>.dump.gpg && pg_restore -U postgres -d restore --no-owner /tmp/r.dump'
docker exec restore-db psql -U postgres -d restore -c 'select count(*) from accounts'; docker rm -f -v restore-db
```
Failure is visible only in `<dir>/backup.log` (cron output); the health probe does not watch backups.

## Rollback
`deploy.sh <host> --rollback` (see README). 2026-10-15 rehearsal on the shared host: with the database at migration 3,
rolling back to a release that predates migration 3 crash-looped the backend (sqlx: "migration 3 was previously
applied but is missing"); that is why `rollback-guard.sh` exists. Rehearsal with a compatible pair (519ec821e90f
<-> 549cc6a3d298): rollback and roll-forward each took ~88 s end to end, but the outage seen from outside was one
failed 0.5 s poll per hop (api 502 once on rollback; relay 502 once on roll-forward) plus smoke checks green.

## E2E matrix
`tests/e2e/run-matrix.sh --scenarios p0,stack --relays local,us,sg` runs the e2e scenarios over relay configs and prints
one summary table (`--list` for names). Not CI: `us`/`sg` are the production relays.
