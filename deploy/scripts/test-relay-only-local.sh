#!/usr/bin/env bash
# Local check of the relay-only stack (compose.relay-only.yml) with Docker: own project and ports,
# builds the relay image, waits for health, runs smoke-check.sh --relay-only, asserts that only the
# relay runs (no backend/Postgres) and that nothing is published beyond loopback, then removes it.
#   deploy/scripts/test-relay-only-local.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export COMPOSE_PROJECT_NAME=fsync-relonly-test FSYNC_TAG=relonly-test
export RELAY_HOST=relay.localhost RELAY_HTTP_PORT="${RELAY_HTTP_PORT:-28081}" RELAY_QUIC_PORT="${RELAY_QUIC_PORT:-27842}"
dc() { docker compose -f "$HERE/compose.relay-only.yml" "$@"; }
cleanup() { dc down -v --remove-orphans >/dev/null 2>&1 || true; }
trap cleanup EXIT
fail() { printf 'FAIL %s\n' "$*" >&2; dc logs --tail 30 relay >&2 || true; exit 1; }

dc config -q
dc build relay
dc up -d --no-build
for _ in $(seq 1 60); do
  st="$(docker inspect -f '{{.State.Health.Status}}' "$(dc ps -q relay)" 2>/dev/null || echo starting)"
  [[ "$st" == "healthy" ]] && break
  sleep 2
done
[[ "$st" == "healthy" ]] || fail "relay not healthy ($st)"

"$HERE/scripts/smoke-check.sh" --relay-only "http://127.0.0.1:$RELAY_HTTP_PORT"

[[ "$(dc ps --services | tr '\n' ' ')" == "relay " ]] || fail "unexpected services: $(dc ps --services | tr '\n' ' ')"
published="$(docker port "$(dc ps -q relay)")"
echo "$published"
# every published binding must be loopback (QAD is off by default)
if grep -v '127\.0\.0\.1' <<<"$published" | grep -q .; then fail "port published beyond loopback"; fi
echo "relay-only local test ok"
