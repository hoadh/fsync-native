#!/usr/bin/env bash
# One-time (idempotent) preparation of a fresh Debian/Ubuntu VPS (AWS Lightsail).
#   deploy/scripts/bootstrap-vps.sh [user@]host --dedicated [--dir /opt/fsync]
# DEDICATED fresh VPS only (it enables ufw, adds swap, installs Docker). --dedicated is mandatory
# and the script still aborts when the host already runs other containers or a web server on 80/443.
# Installs Docker (official script) if missing, opens the host firewall (ufw: 22/tcp, 80/tcp,
# 443/tcp, 7842/udp), adds a 2 GiB swapfile if the host has no swap, creates the stack dir and a
# daily Postgres backup cron job. Needs ssh key access for a sudo-capable user.
# The Lightsail firewall (Networking tab) is separate: open the same ports there (see README).
set -euo pipefail
usage="usage: bootstrap-vps.sh [user@]host --dedicated [--dir /opt/fsync]  (fresh dedicated VPS only)"
host="${1:?$usage}"; shift
dir=/opt/fsync dedicated=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dedicated) dedicated=1; shift;;
    --dir) dir="${2:?--dir needs a path}"; shift 2;;
    *) echo "$usage" >&2; exit 2;;
  esac
done
[[ $dedicated -eq 1 ]] || { echo "refusing: pass --dedicated to confirm $host is a fresh VPS used only for fsync (never a shared host)" >&2; exit 2; }
read -r -a ssh_opts <<<"${SSH_OPTS:--o StrictHostKeyChecking=accept-new}"

ssh "${ssh_opts[@]}" "$host" "sudo env STACK_DIR='$dir' DEPLOY_USER=\"\$(id -un)\" bash -s" <<'REMOTE'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
# Guards: never touch a host that already serves something else.
if command -v docker >/dev/null; then
  others="$(docker ps -a --format '{{.Label "com.docker.compose.project"}}' 2>/dev/null | grep -vx fsync | grep -c . || true)"
  [ "$others" -eq 0 ] || { echo "abort: $others container(s) outside the fsync project exist: shared host" >&2; exit 3; }
fi
if ss -ltn 2>/dev/null | awk '{print $4}' | grep -Eq ':(80|443)$'; then
  echo "abort: something already listens on 80/443: shared host (use deploy.sh with a host Caddy instead)" >&2; exit 3
fi
command -v curl >/dev/null || { apt-get update -qq; apt-get install -y -qq curl ca-certificates; }
if ! command -v docker >/dev/null; then
  curl -fsSL https://get.docker.com | sh
fi
systemctl enable --now docker
id -nG "$DEPLOY_USER" | grep -qw docker || usermod -aG docker "$DEPLOY_USER"

if command -v ufw >/dev/null || apt-get install -y -qq ufw; then
  ufw allow 22/tcp >/dev/null      # before enabling: never lock ourselves out
  ufw allow 80/tcp >/dev/null
  ufw allow 443/tcp >/dev/null
  ufw allow 7842/udp >/dev/null
  ufw --force enable >/dev/null
fi

if [ "$(swapon --show --noheadings | wc -l)" -eq 0 ]; then
  fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile >/dev/null && swapon /swapfile
  grep -q '^/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi

install -d -o "$DEPLOY_USER" -g "$DEPLOY_USER" -m 755 "$STACK_DIR" "$STACK_DIR/scripts"
cat > /etc/cron.d/fsync-backup <<CRON
15 3 * * * $DEPLOY_USER cd $STACK_DIR && ./scripts/backup-postgres.sh >> $STACK_DIR/backup.log 2>&1
CRON
chmod 644 /etc/cron.d/fsync-backup
echo "bootstrap ok: $(docker --version)"
REMOTE
