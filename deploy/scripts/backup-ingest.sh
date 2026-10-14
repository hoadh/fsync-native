#!/usr/bin/env bash
# Runs ON THE BACKUP HOST (cron, every 10 min). The production host's sftp key can only write into
# INCOMING; this script moves finished uploads into BASE/store and prunes there, so a
# compromised production host cannot delete or overwrite stored backups.
#   backup-ingest.sh [BASE]        (default ~/fsync-backups; store/ lives there)
# Env: INCOMING (default BASE/incoming; production: /home/fsyncbak/incoming, the upload-only user's dir),
#      KEEP_DAYS (30), MIN_KEEP (3: never prune below this many files),
#      SETTLE_MIN (10: only files untouched this long are moved, so an upload in flight is never taken).
set -euo pipefail
umask 077
base="${1:-$HOME/fsync-backups}" keep="${KEEP_DAYS:-30}" min_keep="${MIN_KEEP:-3}" settle="${SETTLE_MIN:-10}"
incoming="${INCOMING:-$base/incoming}"
mkdir -p "$incoming" "$base/store"
moved=0
while IFS= read -r f; do
  n="$(basename "$f")"
  # An OpenPGP message starts with a packet header byte with bit 7 set; reject empty/plain files.
  first="$(head -c1 "$f" 2>/dev/null | od -An -tu1 | tr -d ' ' || true)"
  [ -s "$f" ] && [ "${first:-0}" -ge 128 ] || { echo "ingest: skipping unreadable or invalid $n" >&2; continue; }
  [ ! -e "$base/store/$n" ] || { echo "ingest: $n already stored, not overwriting" >&2; continue; }
  mv "$f" "$base/store/$n"; moved=$((moved + 1))
done < <(find "$incoming" -maxdepth 1 -type f -name 'fsync-*.dump.gpg' -mmin "+$settle")
# Uploads that never finished.
find "$incoming" -maxdepth 1 -type f -name '*.partial' -mtime +1 -delete
# Retention by name timestamp (UTC), keeping at least MIN_KEEP files whatever the clock says.
mapfile -t all < <(find "$base/store" -maxdepth 1 -type f -name 'fsync-*.dump.gpg' -exec basename {} \; | sort)
cutoff="fsync-$(date -u -d "-$keep days" +%Y%m%dT%H%M%SZ).dump.gpg"
pruned=0
for ((i = 0; i < ${#all[@]} - min_keep; i++)); do
  [[ "${all[$i]}" < "$cutoff" ]] || break
  rm -f "$base/store/${all[$i]}"; pruned=$((pruned + 1))
done
echo "ingest: moved $moved, pruned $pruned, stored $((${#all[@]} - pruned))"
