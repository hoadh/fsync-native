#!/usr/bin/env bash
# Refuses a backend rollback to a release that does not know the database's migrations: sqlx aborts
# with "migration N was previously applied but is missing", so the old backend crash-loops (seen in
# the 2026-10-15 rehearsal: rollback to d9e5e9b7ef77 after migration 0003 took the API down).
#   rollback-guard.sh RELEASE_TAG DB_MAX_VERSION      (RELEASE_TAG = git sha, optional -dirtyN)
# Reads the target release's backend/migrations from git history (tags are git shas).
set -euo pipefail
tag="${1:?usage: rollback-guard.sh RELEASE_TAG DB_MAX_VERSION}" db_max="${2:?}"
[[ "$db_max" =~ ^[0-9]+$ ]] || { echo "rollback-guard: bad db version '$db_max'" >&2; exit 1; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
rev="${tag%-dirty*}"
files="$(git -C "$ROOT" ls-tree --name-only "$rev" backend/migrations/ 2>/dev/null)" \
  || { echo "rollback-guard: release $tag is not in local git history, cannot verify migrations" >&2; exit 1; }
rel_max="$(printf '%s\n' "$files" | sed -n 's#^backend/migrations/0*\([0-9][0-9]*\)_.*\.up\.sql$#\1#p' | sort -n | tail -1)"
rel_max="${rel_max:-0}"
if ((db_max > rel_max)); then
  echo "rollback-guard: database is at migration $db_max but release $tag only knows up to $rel_max" >&2
  exit 1
fi
echo "rollback-guard: ok (db migration $db_max, release $tag knows $rel_max)"
