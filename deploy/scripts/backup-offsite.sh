#!/usr/bin/env bash
# Nightly backup entry point (cron): local pg_dump via backup-postgres.sh, then an encrypted copy
# is pushed to a second host over sftp. Without a config file it only does the local dump.
#   /opt/fsync/scripts/backup-offsite.sh
# Config: $OFFSITE_ENV (default $STACK_DIR/.backup/offsite.env), plain KEY=value, no secrets:
#   OFFSITE_TARGET=user@host   OFFSITE_DIR=. (the sftp start dir: the backup host's incoming directory)
#   OFFSITE_KEY=path to the ssh private key   OFFSITE_KNOWN_HOSTS=path (pinned host key)
#   GPG_HOME=path to a gpg home holding only the PUBLIC key   GPG_RECIPIENT=key id/uid
#   BACKUP_PING_URL=optional URL pinged after each successful shipment (dead-man's switch)
# The matching private key is NOT kept on this host (see deploy/README.md, "Off-box backups"),
# so a stolen off-box copy is unreadable and a lost host does not take the only decryption key.
set -euo pipefail
umask 077
STACK_DIR="${STACK_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
BACKUP_DIR="${BACKUP_DIR:-$STACK_DIR/backups}"
cfg="${OFFSITE_ENV:-$STACK_DIR/.backup/offsite.env}"
log() { printf '[offsite] %s\n' "$*"; }

"$STACK_DIR/scripts/backup-postgres.sh"
[ -f "$cfg" ] || { log "no $cfg: local backup only"; exit 0; }
# shellcheck disable=SC1090
. "$cfg"
: "${OFFSITE_TARGET:?}" "${OFFSITE_KEY:?}" "${OFFSITE_KNOWN_HOSTS:?}" "${GPG_HOME:?}" "${GPG_RECIPIENT:?}"
dir="${OFFSITE_DIR:-.}"
sftp_do() { sftp -q -b - -i "$OFFSITE_KEY" -o IdentitiesOnly=yes -o BatchMode=yes \
  -o StrictHostKeyChecking=yes -o UserKnownHostsFile="$OFFSITE_KNOWN_HOSTS" -o ConnectTimeout=20 "$OFFSITE_TARGET"; }

dump="$(find "$BACKUP_DIR" -maxdepth 1 -name 'fsync-*.dump' | sort | tail -1)"
[ -s "$dump" ] || { log "no local dump to ship"; exit 1; }
name="$(basename "$dump").gpg"
enc="$BACKUP_DIR/$name"
trap 'rm -f "$enc"' EXIT
gpg --homedir "$GPG_HOME" --batch --yes --trust-model always -r "$GPG_RECIPIENT" -o "$enc" -e "$dump"
size="$(wc -c < "$enc" | tr -d ' ')"

# Upload under a .partial name and rename, so a cut connection never leaves a truncated archive.
# The remote key can only write into the backup host's incoming directory; backup-ingest.sh there
# moves finished files to the store and prunes it (this host cannot delete stored backups).
# chmod 660: the ingest job on the backup host runs as another user in the uploader's group.
printf 'cd "%s"\nput "%s" "%s.partial"\nchmod 660 "%s.partial"\nrename "%s.partial" "%s"\n' "$dir" "$enc" "$name" "$name" "$name" "$name" | sftp_do >/dev/null \
  || { log "FAIL: upload of $name"; exit 1; }
remote="$(printf 'cd "%s"\nls -l "%s"\n' "$dir" "$name" | sftp_do | awk -v n="$name" '$1 ~ /^-/ && $NF ~ ("(^|/)" n "$") {print $5}')" \
  || { log "FAIL: remote listing of $name"; exit 1; }
[ "$remote" = "$size" ] || { log "FAIL: remote size '${remote:-none}' != local $size for $name"; exit 1; }
log "shipped $name ($size bytes) to $OFFSITE_TARGET:$dir"
# Optional dead-man's switch (healthchecks.io style URL): a missed nightly run alerts the owner.
[ -z "${BACKUP_PING_URL:-}" ] || curl -fsS -m 10 --retry 2 -o /dev/null "$BACKUP_PING_URL" || log "warning: ping failed"
