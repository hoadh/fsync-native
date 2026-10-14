#!/usr/bin/env bash
# Prints a deploy/.env with fresh random secrets to stdout. Usage:
#   deploy/scripts/gen-env.sh api.example.com relay.example.com > deploy/.env
# Hostnames are optional (placeholders from .env.example are kept otherwise).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
api="${1:-}" relay="${2:-}"
pg="$(openssl rand -hex 24)"
jwt="$(openssl rand -base64 32)"
sed -e "s|^POSTGRES_PASSWORD=.*|POSTGRES_PASSWORD=${pg}|" \
    -e "s|^JWT_SIGNING_KEY=.*|JWT_SIGNING_KEY=${jwt}|" \
    ${api:+-e "s|^API_HOST=.*|API_HOST=${api}|"} \
    ${relay:+-e "s|^RELAY_HOST=.*|RELAY_HOST=${relay}|"} \
    "$HERE/.env.example"
