#!/usr/bin/env bash
# Deploys the stack (Caddy + backend + relay + Postgres) to a VPS over SSH. Idempotent: re-running
# with the same commit rebuilds/ships nothing and just converges `docker compose up -d`.
#
#   deploy/scripts/deploy.sh [user@]host [options]
#     --env-file F     upload F as the remote .env if the host has none (never overwrites)
#     --rollback       redeploy the previous release (FSYNC_TAG_PREV in the remote .env)
#     --dir D          stack dir on the host (default /opt/fsync)
#     --platform P     image platform for local builds (default linux/amd64)
#     --public-smoke   also smoke-check https://API_HOST and https://RELAY_HOST (needs DNS + proxy)
#     --skip-smoke     skip the on-host smoke check
#     --install-backup-cron  write /etc/cron.d/fsync-backup (daily pg_dump; touches nothing else)
# Env: SSH_OPTS (default "-o StrictHostKeyChecking=accept-new"), DEPLOY_TAG (override image tag).
#
# Default flow: build images locally (buildx, the VPS stays small), `docker save | ssh docker load`
# only for tags the host lacks, copy compose files, update FSYNC_TAG in the remote .env, `up -d`,
# wait for container health, smoke-check over loopback. Releases are immutable git-sha tags;
# the last two are kept on the host.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HERE="$ROOT/deploy"
host="" env_file="" cron=0 rollback=0 dir=/opt/fsync platform=linux/amd64 smoke=1 public=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file) env_file="${2:?}"; shift 2;;
    --install-backup-cron) cron=1; shift;;
    --rollback) rollback=1; shift;;
    --dir) dir="${2:?}"; shift 2;;
    --platform) platform="${2:?}"; shift 2;;
    --public-smoke) public=1; shift;;
    --skip-smoke) smoke=0; shift;;
    -*) echo "unknown option $1" >&2; exit 2;;
    *) host="$1"; shift;;
  esac
done
[[ -n "$host" ]] || { sed -n 2,14p "$0"; exit 2; }
read -r -a ssh_opts <<<"${SSH_OPTS:--o StrictHostKeyChecking=accept-new}"
log() { printf '[deploy] %s\n' "$*"; }
die() { printf '[deploy] FAIL: %s\n' "$*" >&2; exit 1; }
rsh() { ssh "${ssh_opts[@]}" "$host" "$@"; }

# docker may need sudo when the ssh user is not in the docker group (shared hosts).
if rsh "docker info >/dev/null 2>&1"; then D=docker; elif rsh "sudo -n docker info >/dev/null 2>&1"; then D="sudo -n docker"
else die "no docker access on $host (run deploy/scripts/bootstrap-vps.sh $host, or allow sudo docker)"; fi
rsh "$D compose version >/dev/null" || die "docker compose missing on $host"
rsh "test -d '$dir' && test -w '$dir' || { sudo -n install -d -o \"\$(id -un)\" -m 750 '$dir'; }"

# --- release tag
if [[ $rollback -eq 1 ]]; then
  tag="$(rsh "sed -n 's/^FSYNC_TAG_PREV=//p' '$dir/.env'")"
  [[ -n "$tag" ]] || die "no previous release recorded on the host"
  log "rolling back to $tag"
else
  tag="${DEPLOY_TAG:-$(git -C "$ROOT" rev-parse --short=12 HEAD)}"
  if [[ -z "${DEPLOY_TAG:-}" && -n "$(git -C "$ROOT" status --porcelain -- core backend deploy Cargo.toml Cargo.lock)" ]]; then
    tag="$tag-dirty$(date +%s)"
    log "working tree has uncommitted deploy inputs: using tag $tag"
  fi
fi

have_image() { rsh "$D image inspect fsync-backend:$tag fsync-relay:$tag >/dev/null 2>&1"; }

# --- images
if [[ $rollback -eq 0 ]] && ! have_image; then
  # Built locally (cross-compiled, see the Dockerfiles): a 2 GiB shared VPS must never compile Rust.
  log "building $platform images locally"
  docker buildx build --platform "$platform" --load -t "fsync-backend:$tag" \
    -f "$ROOT/deploy/backend/Dockerfile" "$ROOT"
  docker buildx build --platform "$platform" --load -t "fsync-relay:$tag" "$HERE/relay"
  log "shipping images"
  docker save "fsync-backend:$tag" "fsync-relay:$tag" | gzip | rsh "gunzip | $D load"
elif [[ $rollback -eq 1 ]]; then
  have_image || die "images for $tag are gone from the host"
else
  log "images for $tag already on host"
fi

# --- files, env
stage=""
if [[ $rollback -eq 0 ]]; then
  log "copying compose files"
  # Unique staging dir: a concurrent deploy cannot clobber it before it takes the lock below.
  stage="$(rsh "mktemp -d '$dir/.incoming.XXXXXX'")"
  trap 'rsh "rm -rf \"$stage\"" >/dev/null 2>&1 || true' EXIT
  COPYFILE_DISABLE=1 tar -C "$HERE" -cf - compose.yml Caddyfile scripts | rsh "tar -C '$stage' -xf -"
fi
if ! rsh "test -s '$dir/.env'"; then
  [[ -n "$env_file" ]] || die "no $dir/.env on the host: pass --env-file (see deploy/scripts/gen-env.sh)"
  rsh "umask 077 && cat > '$dir/.env.new' && mv '$dir/.env.new' '$dir/.env'" < "$env_file"
  log "uploaded .env"
fi
rsh "chmod 600 '$dir/.env'"

# --- switch release (one deploy at a time; .env edited atomically; compose.yml.prev kept for rollback)
rsh "bash -s '$dir' '$tag' '$D' '$rollback' '$stage'" <<'REMOTE'
set -euo pipefail
dir="$1" tag="$2" D="$3 "; D="${D% }"; rollback="$4" stage="$5"; cd "$dir"
exec 9>.deploy.lock
flock -n 9 || { echo "another deploy is running on this host" >&2; exit 1; }
umask 022   # compose.yml, Caddyfile snippet and scripts are not secret; only .env is (below)
# stale macOS AppleDouble files from earlier tar uploads (only directly in these two dirs)
find "$dir" "$dir/scripts" -maxdepth 1 -type f -name '._*' -delete 2>/dev/null || true
cur="$(sed -n 's/^FSYNC_TAG=//p' .env | tail -1)"
if [ "$rollback" = 1 ]; then
  if [ -f compose.yml.prev ]; then mv compose.yml compose.yml.tmp; mv compose.yml.prev compose.yml; mv compose.yml.tmp compose.yml.prev; fi
else
  [ "$cur" = "$tag" ] || { [ ! -f compose.yml ] || cp compose.yml compose.yml.prev; }
  cp "$stage/compose.yml" compose.yml.new && mv compose.yml.new compose.yml
  cp "$stage/Caddyfile" Caddyfile; mkdir -p scripts; cp -r "$stage/scripts/." scripts/; rm -rf "$stage"
  chmod 644 compose.yml Caddyfile; chmod 755 scripts; chmod 755 scripts/*.sh
fi
umask 077
{
  grep -v -e '^FSYNC_TAG=' -e '^FSYNC_TAG_PREV=' .env || true
  prev="$cur"; [ "$cur" != "$tag" ] || prev="$(sed -n 's/^FSYNC_TAG_PREV=//p' .env | tail -1)"
  [ -z "$prev" ] || echo "FSYNC_TAG_PREV=$prev"
  echo "FSYNC_TAG=$tag"
} > .env.new
# (grep re-terminates a last line that lacks its newline, so appended keys never merge into it)
mv .env.new .env; chmod 600 .env
$D compose up -d --no-build --remove-orphans
for _ in $(seq 1 90); do
  bad=0
  for s in postgres backend relay; do
    cid="$($D compose ps -q "$s")"
    [ -n "$cid" ] || { bad=1; continue; }
    st="$($D inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$cid" 2>/dev/null || echo missing)"
    case "$st" in healthy|running) ;; *) bad=1;; esac
  done
  [ "$bad" -eq 0 ] && break
  sleep 2
done
$D compose ps
[ "$bad" -eq 0 ] || { $D compose logs --tail 30 >&2; echo "containers not healthy" >&2; exit 1; }
# keep current + previous release images, drop older ones
keep="$(sed -n 's/^FSYNC_TAG_PREV=//p' .env | tail -1)"
for img in fsync-backend fsync-relay; do
  $D images "$img" --format '{{.Tag}}' | grep -vx -e "$tag" -e "${keep:-none}" -e local | xargs -r -I{} $D rmi "$img:{}" || true
done
REMOTE

# --- backups: dumps are scheduled by a cron.d file scoped to this stack (the only host file we write)
if [[ $cron -eq 1 ]]; then
  rsh "printf '15 3 * * * %s cd %s && ./scripts/backup-postgres.sh >> %s/backup.log 2>&1\n' \"\$(id -un)\" '$dir' '$dir' \
    | sudo -n tee /etc/cron.d/fsync-backup >/dev/null && sudo -n chmod 644 /etc/cron.d/fsync-backup"
  log "backup cron installed: /etc/cron.d/fsync-backup (daily 03:15, 14 days kept)"
elif ! rsh "test -f /etc/cron.d/fsync-backup"; then
  log "WARNING: backups are NOT scheduled on this host (rerun with --install-backup-cron)"
fi

# --- smoke (on the host over loopback; the public endpoints need DNS + the proxy snippet)
if [[ $smoke -eq 1 ]]; then
  bp="$(rsh "sed -n 's/^BACKEND_PORT=//p' '$dir/.env'")"; rp="$(rsh "sed -n 's/^RELAY_HTTP_PORT=//p' '$dir/.env'")"
  log "on-host smoke check (127.0.0.1:${bp:-18080} / ${rp:-18081})"
  rsh "CHECK_PROXY_TRUST=1 OOM_CHECK_DOCKER='$D' bash -s -- http://127.0.0.1:${bp:-18080} http://127.0.0.1:${rp:-18081}" < "$HERE/scripts/smoke-check.sh" \
    || die "smoke check failed (release $tag is running; inspect: ssh $host 'cd $dir && $D compose logs')"
fi
if [[ $public -eq 1 ]]; then
  api="$(rsh "sed -n 's/^API_HOST=//p' '$dir/.env'")"; relay="$(rsh "sed -n 's/^RELAY_HOST=//p' '$dir/.env'")"
  log "public smoke check https://$api https://$relay"
  "$HERE/scripts/smoke-check.sh" "https://$api" "https://$relay" || die "public smoke check failed"
fi
log "deployed $tag"
