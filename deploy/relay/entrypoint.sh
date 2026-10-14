#!/bin/sh
# Renders the iroh-relay config and starts it.
#
# TLS is terminated by the reverse proxy (Caddy) in front: the relay speaks plain HTTP on :3340
# (dangerous_http_only, the behind-a-proxy mode). With RELAY_QAD=1 it also runs QUIC address
# discovery on udp/7842, which needs a certificate for RELAY_HOSTNAME: it reads the one Caddy
# manages from /caddy-data (re-read periodically, so renewals are picked up).
set -eu
: "${RELAY_HOSTNAME:?RELAY_HOSTNAME is required}"
CONF=/tmp/iroh-relay.toml

cat > "$CONF" <<TOML
enable_relay = true
http_bind_addr = "0.0.0.0:3340"
enable_metrics = true
metrics_bind_addr = "0.0.0.0:9090"
TOML

if [ "${RELAY_QAD:-0}" = "1" ]; then
  # Caddy obtains the certificate shortly after it first sees the hostname; wait for it
  # (compose restarts us if it never shows up). Layout-agnostic: docker volume or host path.
  i=0; CRT=""
  while [ -z "$CRT" ]; do
    CRT="$(find /caddy-data -path "*/certificates/*/${RELAY_HOSTNAME}/${RELAY_HOSTNAME}.crt" 2>/dev/null | head -1)"
    [ -n "$CRT" ] && break
    i=$((i + 1))
    if [ "$i" -gt 120 ]; then echo "no certificate for ${RELAY_HOSTNAME} under /caddy-data after 120 s" >&2; exit 1; fi
    sleep 1
  done
  KEY="${CRT%.crt}.key"
  cat >> "$CONF" <<TOML
enable_quic_addr_discovery = true

[tls]
cert_mode = "Reloading"
hostname = "${RELAY_HOSTNAME}"
manual_cert_path = "${CRT}"
manual_key_path = "${KEY}"
quic_bind_addr = "0.0.0.0:7842"
dangerous_http_only = true
TOML
fi

cat >> "$CONF" <<TOML

[limits]
accept_conn_limit = 50.0
accept_conn_burst = 200
TOML
if [ -n "${RELAY_RX_BYTES_PER_SEC:-}" ]; then
  printf '\n[limits.client.rx]\nbytes_per_second = %s\n' "$RELAY_RX_BYTES_PER_SEC" >> "$CONF"
fi
exec iroh-relay --config-path "$CONF"
