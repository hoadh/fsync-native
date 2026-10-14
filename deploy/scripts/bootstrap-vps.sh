#!/usr/bin/env bash
# One-time (idempotent) preparation of a fresh Debian/Ubuntu VPS (AWS Lightsail).
#   deploy/scripts/bootstrap-vps.sh [user@]host [--dir /opt/fsync]
# Installs Docker (official script) if missing, opens the host firewall (ufw: 22/tcp, 80/tcp,
# 443/tcp, 7842/udp), adds a 2 GiB swapfile if the host has no swap, creates the stack dir and a
# daily Postgres backup cron job. Needs ssh key access for a sudo-capable user.
# The Lightsail firewall (Networking tab) is separate: open the same ports there (see README).
set -euo pipefail
host="${1:?usage: bootstrap-vps.sh [user@]host [--dir /opt/fsync]}"; shift
dir=/opt/fsync
[[ "${1:-}" == "--dir" ]] && dir="${2:?--dir needs a path}"
read -r -a ssh_opts <<<"${SSH_OPTS:--o StrictHostKeyChecking=accept-new}"

ssh "${ssh_opts[@]}" "$host" "sudo env STACK_DIR='$dir' DEPLOY_USER=\"\$(id -un)\" bash -s" <<'REMOTE'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
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
