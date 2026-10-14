#!/usr/bin/env bash
# Restores a dump made by backup-postgres.sh into the running stack's database (replaces objects).
#   /opt/fsync/scripts/restore-postgres.sh backups/fsync-20261014T150841Z.dump
# User/database come from the postgres container's own environment (the stack .env), nothing is
# hardcoded. Stop the backend first for a consistent restore: docker compose stop backend.
set -euo pipefail
dump="${1:?usage: restore-postgres.sh DUMP_FILE}"
[ -s "$dump" ] || { echo "no such dump: $dump" >&2; exit 1; }
[ "$(head -c 5 "$dump")" = "PGDMP" ] || { echo "not a pg_dump custom-format archive: $dump" >&2; exit 1; }
STACK_DIR="${STACK_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$STACK_DIR"
if docker info >/dev/null 2>&1; then D=(docker); else D=(sudo -n docker); fi
"${D[@]}" compose exec -T postgres sh -c 'pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --clean --if-exists --no-owner' < "$dump"
echo "restore ok: $dump"
