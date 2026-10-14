#!/usr/bin/env bash
# Smoke check of a deployed stack.  Usage: smoke-check.sh API_BASE RELAY_BASE
#   public:   smoke-check.sh https://api-fsync.10z.one https://relay-fsync.10z.one
#   on host:  smoke-check.sh http://127.0.0.1:18080 http://127.0.0.1:18081   (deploy.sh does this over ssh)
# Checks: API /health (also runs SELECT 1), relay HTTP(S) endpoint, relay websocket path answers an
# upgrade attempt. Env: CURL_EXTRA (extra curl args, e.g. "--cacert ca.pem --resolve ..."),
# QUIC_HOST + QUIC_PORT (default 7842): best-effort UDP probe of relay QAD (UDP gives no ack).
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
  "$relay/relay" || true)"
case "$code" in 101|400|401|403|426) ok "relay /relay answers upgrade (HTTP $code)";; *) bad "relay /relay HTTP '$code'";; esac

if [[ -n "${QUIC_HOST:-}" ]] && command -v nc >/dev/null; then
  if nc -zu -w 2 "$QUIC_HOST" "${QUIC_PORT:-7842}" 2>/dev/null; then ok "relay udp/${QUIC_PORT:-7842} (best effort)"
  else echo "warn relay udp/${QUIC_PORT:-7842} not confirmed (firewall? UDP gives no ack)"; fi
fi
echo "smoke ok"
