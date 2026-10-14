# Operations: health probe, off-box backups, rollback

## Health probe and alerts
`.github/workflows/health-probe.yml` runs `scripts/probe-health.sh` hourly (private repo: one run bills a
minute, ~720 of 2000 free minutes a month; edit the cron for faster). It checks `api-fsync.10z.one/health`,
`/health/otp`, `relay-fsync.10z.one/healthz`, `relay-sg-fsync.10z.one/healthz` and that every certificate has
>= 14 days left. A failed scheduled run notifies the user who last edited the cron line (GitHub notification settings: Actions;
scheduled runs can also be delayed under load). Run it by hand: `deploy/scripts/probe-health.sh`.

`/health/otp` is the "global OTP send cap reached" signal: 200 `ok`, or 503 `otp cap reached` while
`OTP_DAILY_SEND_CAP` sends happened in the last 24 h (sign-ins are refused until the oldest slot expires); the probe keeps failing, so it repeats hourly until then.
The backend also logs `WARN global OTP send cap reached; start refused` on every refused start
(`docker compose logs backend | grep "OTP send cap"`). A release without the endpoint answers 404: the probe warns only.

## Off-box Postgres backups
Cron on the app host (`/etc/cron.d/fsync-backup`, 03:15) runs `scripts/backup-offsite.sh`: `pg_dump` into `backups/` (14 days), then
gpg-encrypts the dump and uploads it by sftp (size verified) to a backup host. Config: `<dir>/.backup/offsite.env` (no secrets; keys in
the script header), ssh key `.backup/id_ed25519`, pinned `.backup/known_hosts`, gpg home `.backup/gnupg` holding **only the public key**.

Backup host (the Singapore VPS): a dedicated user `fsyncbak` (no sudo) whose only authorized key is
`restrict,command="internal-sftp -d /home/fsyncbak/incoming -u 007"` (`authorized_keys` is root-owned, so it cannot be edited by that
user). `scripts/backup-ingest.sh` (cron.d, every 10 min, as the normal user, which is in group `fsyncbak`) moves finished uploads to
`~/fsync-backups/store`, which the upload user cannot enter, and prunes there (30 days, never below 3 files, never while the newest
stored backup is itself older than 30 days; names with a future timestamp or an existing name go to `rejected/`). So a compromised app
host can add files but not delete or replace stored backups. The upload user can still read world-readable files on the backup host.
Provisioning on the backup host: `useradd -m fsyncbak`, `usermod -aG fsyncbak <normal user>`, `chmod 750 /home/fsyncbak`,
`install -d -o fsyncbak -g fsyncbak -m 2770 /home/fsyncbak/incoming`, root-owned `/home/fsyncbak/.ssh/authorized_keys` with the
restricted line, and `/etc/cron.d/fsync-backup-ingest`:
`*/10 * * * * <normal user> INCOMING=/home/fsyncbak/incoming /home/<normal user>/fsync-backups/backup-ingest.sh >> /home/<normal user>/fsync-backups/ingest.log 2>&1`.
`BACKUP_PING_URL` in `offsite.env` (optional, a secret: keep the file 600, e.g. a healthchecks.io check) is pinged after each successful shipment: without it a failed
night is visible only in `<dir>/backup.log`.

The private key is generated once and kept off both hosts (owner's password manager); losing it makes the off-box copies
unreadable. Restore drill (any machine with docker; production is never touched):
```bash
scp <backup-host>:fsync-backups/store/fsync-<ts>.dump.gpg .
docker run -d --name restore-db -e POSTGRES_PASSWORD=x -e POSTGRES_DB=restore -v "$PWD:/r" postgres:17
docker exec restore-db sh -c 'export GNUPGHOME=$(mktemp -d); gpg --batch --import /r/<secret.asc> &&
  gpg --batch -d -o /tmp/r.dump /r/fsync-<ts>.dump.gpg && pg_restore -U postgres -d restore --no-owner /tmp/r.dump'
docker exec restore-db psql -U postgres -d restore -c 'select count(*) from accounts'; docker rm -f -v restore-db
```

## Rollback
`deploy.sh <host> --rollback` (see README). 2026-10-15 rehearsal on the shared host: with the database at migration 3,
rolling back to a release that predates migration 3 crash-looped the backend (sqlx: "migration 3 was previously
applied but is missing"); that is why `rollback-guard.sh` exists. Rehearsal with a compatible pair (519ec821e90f
<-> 549cc6a3d298): rollback and roll-forward each took ~88 s end to end, but the outage seen from outside was one
failed 0.5 s poll per hop (api 502 once on rollback; relay 502 once on roll-forward) plus smoke checks green.

## E2E matrix
`tests/e2e/run-matrix.sh --scenarios p0,stack --relays local,us,sg` runs the e2e scenarios over relay configs and prints
one summary table (`--list` for names). Not CI: `us`/`sg` are the production relays.
