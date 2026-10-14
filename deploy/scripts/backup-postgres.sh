#!/usr/bin/env bash
# Dumps the fsync database (pg_dump custom format) into BACKUP_DIR and prunes old dumps.
# Runs on the VPS from cron (installed by bootstrap-vps.sh), from the stack directory:
#   /opt/fsync/scripts/backup-postgres.sh
# Env: STACK_DIR (default: parent of this script), BACKUP_DIR (default $STACK_DIR/backups),
#      KEEP_DAYS (default 14).
# Restore: docker compose exec -T postgres pg_restore -U fsync -d fsync --clean --if-exists < dump
set -euo pipefail
STACK_DIR="${STACK_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
BACKUP_DIR="${BACKUP_DIR:-$STACK_DIR/backups}"
KEEP_DAYS="${KEEP_DAYS:-14}"
cd "$STACK_DIR"
mkdir -p "$BACKUP_DIR"
chmod 700 "$BACKUP_DIR"

out="$BACKUP_DIR/fsync-$(date -u +%Y%m%dT%H%M%SZ).dump"
tmp="$out.partial"
trap 'rm -f "$tmp"' EXIT
docker compose exec -T postgres sh -c 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" -Fc' > "$tmp"
# A valid custom-format archive starts with the "PGDMP" magic; an empty/failed dump does not.
[ "$(head -c 5 "$tmp")" = "PGDMP" ] || { echo "backup failed: not a pg_dump archive" >&2; exit 1; }
mv "$tmp" "$out"
find "$BACKUP_DIR" -name 'fsync-*.dump' -mtime "+$KEEP_DAYS" -delete
echo "backup ok: $out ($(wc -c < "$out") bytes)"
