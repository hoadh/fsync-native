#!/usr/bin/env bash
# Nightly backup entry point (cron): local pg_dump via backup-postgres.sh, then an encrypted copy
# is pushed to a second host over sftp. Without a config file it only does the local dump.
#   /opt/fsync/scripts/backup-offsite.sh
# Config: $OFFSITE_ENV (default $STACK_DIR/.backup/offsite.env), plain KEY=value, no secrets:
#   OFFSITE_TARGET=user@host   OFFSITE_DIR=fsync-backups (relative to the remote home)
#   OFFSITE_KEY=path to the ssh private key   OFFSITE_KNOWN_HOSTS=path (pinned host key)
#   GPG_HOME=path to a gpg home holding only the PUBLIC key   GPG_RECIPIENT=key id/uid
#   KEEP_REMOTE_DAYS=30
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
dir="${OFFSITE_DIR:-fsync-backups}" keep="${KEEP_REMOTE_DAYS:-30}"
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
printf 'cd %s\nput %s %s.partial\nrename %s.partial %s\n' "$dir" "$enc" "$name" "$name" "$name" | sftp_do >/dev/null
remote="$(printf 'cd %s\nls -l %s\n' "$dir" "$name" | sftp_do | awk -v n="$name" '$1 ~ /^-/ && $NF == n {print $5}')"
[ "$remote" = "$size" ] || { log "FAIL: remote size '${remote:-none}' != local $size for $name"; exit 1; }
log "shipped $name ($size bytes) to $OFFSITE_TARGET:$dir"

# Retention on the remote side: names carry the UTC timestamp, so a string compare is a date compare.
cutoff="fsync-$(date -u -d "-$keep days" +%Y%m%dT%H%M%SZ).dump.gpg"
old="$(printf 'cd %s\nls -1\n' "$dir" | sftp_do | sed -n 's#^.*/##;/^fsync-[0-9TZ]*\.dump\.gpg$/p' | sort | awk -v c="$cutoff" '$0 < c')"
if [ -n "$old" ]; then
  for f in $old; do printf 'cd %s\nrm %s\n' "$dir" "$f"; done | sftp_do >/dev/null
  log "pruned remote: $(echo "$old" | wc -l | tr -d ' ') file(s) older than $keep days"
fi
