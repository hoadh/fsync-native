#!/bin/sh
# Copies the relay hostname's certificate, which the host Caddy manages, into a directory that only
# the relay container user (uid 10001) can read. QUIC address discovery (RELAY_QAD=1) then runs
# without RELAY_USER=0 and without mounting anything of Caddy's. Never touches Caddy or its config.
#
# Runs as root from a root-owned path (cron must not run a file the deploy user can rewrite):
#   sudo install -o root -g root -m 755 scripts/sync-relay-cert.sh /usr/local/sbin/fsync-sync-relay-cert
#   sudo fsync-sync-relay-cert relay-sg-fsync.10z.one          # -> /opt/fsync/relay-cert, then daily cron
# Caddy renews about 30 days before expiry; the relay re-reads the files (cert_mode "Reloading").
# Env: CADDY_CERTS (Caddy's certificates root), RELAY_UID (default 10001).
# Output: one line on change; errors and a near-expiry warning on stderr and syslog (fsync-relay-cert).
# Exit: 0 ok, 1 error (old copy kept), 3 copy expires within 14 days. Needs GNU date (Debian/Ubuntu).
set -eu
host="${1:-}"
dest="${2:-/opt/fsync/relay-cert}"
root="${CADDY_CERTS:-/var/lib/caddy/.local/share/caddy/certificates}"
uid="${RELAY_UID:-10001}"

log() { logger -t fsync-relay-cert "$*" 2>/dev/null || true; }
die() { echo "sync-relay-cert: $*" >&2; log "error: $*"; exit 1; }
# The hostname becomes a file name written as root: no "/" or anything else unexpected.
case "$host" in "" | *[!A-Za-z0-9.-]* | .*) die "usage: sync-relay-cert.sh <relay hostname> [dest dir]" ;; esac

# A usable pair: regular files, > 1 day left, key matches the certificate. Prints the expiry (epoch).
# Caddy writes crt and key separately, so a pair caught mid-renewal fails and waits for the next run.
usable() {
  [ -f "$1" ] && [ ! -L "$1" ] && [ -f "$2" ] && [ ! -L "$2" ] || return 1
  openssl x509 -in "$1" -noout -checkend 86400 >/dev/null 2>&1 || return 1
  [ "$(openssl x509 -in "$1" -noout -pubkey | openssl sha256)" = "$(openssl pkey -in "$2" -pubout | openssl sha256)" ] || return 1
  date -d "$(openssl x509 -in "$1" -noout -enddate | cut -d= -f2)" +%s
}

# Caddy keeps one directory per ACME issuer; after an issuer switch a stale one can remain. Take the
# usable pair with the latest expiry.
best="" best_end=0
for d in $(find "$root" -type d -name "$host"); do
  end="$(usable "$d/$host.crt" "$d/$host.key")" || continue
  if [ "$end" -gt "$best_end" ]; then best="$d" best_end="$end"; fi
done
[ -n "$best" ] || die "no valid certificate with a matching key for $host under $root"

# root-owned, group-readable by the relay: nothing uid 10001 controls can change the directory.
install -d -o 0 -g "$uid" -m 0550 "$dest"
ct="$dest/.$host.crt.tmp" kt="$dest/.$host.key.tmp"
trap 'rm -f "$ct" "$kt"' EXIT
# Stage, then validate what was copied: Caddy may renew between the scan and the copy.
install -o 0 -g "$uid" -m 0440 "$best/$host.crt" "$ct"
install -o 0 -g "$uid" -m 0440 "$best/$host.key" "$kt"
end="$(usable "$ct" "$kt")" || die "the pair changed while copying; old copy kept, retry next run"
if cmp -s "$ct" "$dest/$host.crt" && cmp -s "$kt" "$dest/$host.key"; then
  chown 0:"$uid" "$dest/$host.crt" "$dest/$host.key"
  chmod 0440 "$dest/$host.crt" "$dest/$host.key"
else
  # Rename both: the relay never reads a half-written file. A reload between the two renames sees
  # a mismatched pair, fails, keeps the last good one and succeeds on the next reload interval.
  mv -f "$kt" "$dest/$host.key"
  mv -f "$ct" "$dest/$host.crt"
  echo "updated $dest/$host.crt and .key"
  log "updated $dest/$host.crt (expires $(date -d "@$end" -u +%F))"
fi
# Caddy should have renewed by now; a copy this close to expiry means renewal or this sync broke.
if ! openssl x509 -in "$dest/$host.crt" -noout -checkend $((14 * 86400)) >/dev/null; then
  echo "sync-relay-cert: WARNING $host certificate expires within 14 days" >&2
  log "warning: $host certificate expires within 14 days"
  exit 3
fi
