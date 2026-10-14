#!/usr/bin/env bash
# Smoke check of a deployed stack.  Usage: smoke-check.sh API_BASE RELAY_BASE
#   public:   smoke-check.sh https://api-fsync.10z.one https://relay-fsync.10z.one
#   on host:  smoke-check.sh http://127.0.0.1:18080 http://127.0.0.1:18081   (deploy.sh does this over ssh)
# Checks: API /health (also runs SELECT 1), relay HTTP(S) endpoint, relay websocket path answers an
# upgrade attempt. Env: CURL_EXTRA (extra curl args, e.g. "--cacert ca.pem --resolve ..."),
# QUIC_HOST + QUIC_PORT (default 7842): best-effort UDP probe of relay QAD (UDP gives no ack).
# CHECK_PROXY_TRUST=1 (only against the backend's own loopback port): proves the backend trusts the
# proxy peer it is reached through, i.e. honors X-Forwarded-For (see check below).
set -euo pipefail
api="${1:?usage: smoke-check.sh API_BASE RELAY_BASE}" relay="${2:?relay base url}"
# shellcheck disable=SC2206
extra=(${CURL_EXTRA:-})
c() { curl -sS --max-time 10 ${extra[@]+"${extra[@]}"} "$@"; }
ok() { printf 'ok   %s\n' "$*"; }
bad() { printf 'FAIL %s\n' "$*" >&2; exit 1; }

body="$(c "$api/health")" || bad "api $api/health unreachable"
[[ "$body" == "ok" ]] || bad "api health body '$body'"
ok "api /health"

c -o /dev/null -f "$relay/ping" || c -o /dev/null -f "$relay/" || bad "relay $relay unreachable"
ok "relay http"

code="$(c -o /dev/null -w '%{http_code}' -H 'Connection: Upgrade' -H 'Upgrade: websocket' \
  -H 'Sec-WebSocket-Version: 13' -H 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==' \
  -m 3 "$relay/relay" || true)"
case "$code" in 101|400|401|403|426) ok "relay /relay answers upgrade (HTTP $code)";; *) bad "relay /relay HTTP '$code'";; esac

if [[ -n "${QUIC_HOST:-}" ]] && command -v nc >/dev/null; then
  if nc -zu -w 2 "$QUIC_HOST" "${QUIC_PORT:-7842}" 2>/dev/null; then ok "relay udp/${QUIC_PORT:-7842} (best effort)"
  else echo "warn relay udp/${QUIC_PORT:-7842} not confirmed (firewall? UDP gives no ack)"; fi
fi
# Per-IP limits only work if the backend resolves the client from X-Forwarded-For. Open more
# unauthenticated presence sockets than MAX_UNAUTH_WS_PER_IP allows per IP, each claiming a distinct
# client in X-Forwarded-For: all are admitted only when the peer we come through is trusted
# (TRUSTED_PROXIES). If it is not, they share one bucket and the surplus is refused.
if [[ "${CHECK_PROXY_TRUST:-0}" == "1" ]]; then
  python3 -I - "$api" "${PROXY_PROBES:-14}" <<'PY' || bad "backend does not trust the proxy peer (check TRUSTED_PROXIES vs the compose subnet)"
import socket, sys, urllib.parse
u = urllib.parse.urlparse(sys.argv[1]); n = int(sys.argv[2]); socks = []; ok = 0
for i in range(n):
    s = socket.create_connection((u.hostname, u.port or 80), timeout=5)
    s.sendall((f"GET /v1/presence HTTP/1.1\r\nHost: {u.netloc}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n"
               f"Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
               f"X-Forwarded-For: 198.51.100.{i + 1}\r\n\r\n").encode())
    line = s.recv(64).split(b"\r\n")[0]
    socks.append(s); ok += b" 101 " in line
sys.exit(0 if ok == n else 1)
PY
  ok "backend trusts the proxy peer (X-Forwarded-For honored)"
fi
echo "smoke ok"
