#!/usr/bin/env bash
# Local verification of the production deploy stack (deploy/compose.yml + compose.local.yml) with
# Docker:  compose up (backend + relay + Postgres, plus a Caddy with a local CA running the same
#   deploy/Caddyfile the host Caddy uses) -> smoke over loopback HTTP and over HTTPS via Caddy ->
#   two fsync-cli devices: login, pair, clip A->B direct, then clip + file A->B forced through the
#   composed relay -> Postgres backup + restore -> compose down -v.
#
# Own project name and free ports; never touches other containers. Needs docker, cargo, python3.
# Staging mode (FSYNC_ENV=staging + dev mode) is used only so the OTP can be read from the
# backend log: there is no email provider yet. Honors CARGO_TARGET_DIR, KEEP=1.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD="${CARGO_TARGET_DIR:-$ROOT/target}"
mkdir -p "$BUILD" && BUILD="$(cd "$BUILD" && pwd)"
DEPLOY="$ROOT/deploy"
WORK="$(mktemp -d)"
PROJECT="fsync-e2e-$$"
ENVF="$WORK/env"
PIDS=()
log() { printf '[stack-e2e] %s\n' "$*"; }
fail() { printf '[stack-e2e] FAIL: %s\n' "$*" >&2; exit 1; }
free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
dc() { docker compose -p "$PROJECT" --env-file "$ENVF" -f "$DEPLOY/compose.yml" -f "$DEPLOY/compose.local.yml" "$@"; }

cleanup() {
  local rc=$?
  for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done
  if [[ $rc -ne 0 && -f "$ENVF" ]]; then dc logs --tail 25 2>&1 | sed 's/^/  | /' || true; fi
  if [[ -f "$ENVF" ]]; then dc down -v --remove-orphans >/dev/null 2>&1 || true; fi
  if [[ -z "${KEEP:-}" ]]; then rm -rf "$WORK"; else log "work dir kept: $WORK"; fi
  exit $rc
}
trap cleanup EXIT INT TERM

BACKEND_PORT="$(free_port)"; RELAY_HTTP_PORT="$(free_port)"; LOCAL_HTTPS_PORT="$(free_port)"
QUIC_PORT="$(free_port)"
# Base env from the template, then local-test overrides (later assignments win in compose).
"$DEPLOY/scripts/gen-env.sh" api.localhost relay.localhost >"$ENVF"
cat >>"$ENVF" <<ENV
FSYNC_ENV=staging
FSYNC_DEV_MODE=1
RUST_LOG=fsync_backend=debug,info
BACKEND_PORT=$BACKEND_PORT
RELAY_HTTP_PORT=$RELAY_HTTP_PORT
LOCAL_HTTPS_PORT=$LOCAL_HTTPS_PORT
RELAY_QAD=1
RELAY_QUIC_PORT=$QUIC_PORT
ENV

log "building CLI"
(cd "$ROOT" && cargo build --release -p fsync-cli)
CLI="$BUILD/release/fsync-cli"; [[ -x "$CLI" ]] || fail "missing $CLI"

log "compose up (project $PROJECT)"
dc up -d --build --wait --wait-timeout 900 || fail "compose up"
dc ps

log "smoke: loopback HTTP (what deploy.sh checks on the host)"
"$DEPLOY/scripts/smoke-check.sh" "http://127.0.0.1:$BACKEND_PORT" "http://127.0.0.1:$RELAY_HTTP_PORT"
log "smoke: HTTPS through Caddy (local CA)"
dc cp caddy:/data/caddy/pki/authorities/local/root.crt "$WORK/ca.pem" >/dev/null
CURL_EXTRA="--cacert $WORK/ca.pem --resolve api.localhost:$LOCAL_HTTPS_PORT:127.0.0.1 --resolve relay.localhost:$LOCAL_HTTPS_PORT:127.0.0.1" \
  QUIC_HOST=127.0.0.1 QUIC_PORT="$QUIC_PORT" \
  "$DEPLOY/scripts/smoke-check.sh" "https://api.localhost:$LOCAL_HTTPS_PORT" "https://relay.localhost:$LOCAL_HTTPS_PORT"
dc logs relay 2>&1 | grep -qi "QUIC server listening" || fail "relay log shows no QUIC server"

API="http://127.0.0.1:$BACKEND_PORT"; RELAY="http://127.0.0.1:$RELAY_HTTP_PORT"
EXTRA=()
cli() {
  local n="$1"; shift
  "$CLI" --data "$WORK/$n" --api "$API" --relay "$RELAY" --insecure-http --name "dev$n" \
    ${EXTRA[@]+"${EXTRA[@]}"} "$@"
}
jfield() {
  python3 -I - "$1" "$2" "$3" <<'PY'
import json, sys
path, ev, field = sys.argv[1:4]; val = None
for line in open(path):
    try: o = json.loads(line)
    except ValueError: continue
    if o.get("event") == ev and field in o: val = o[field]
print("" if val is None else val)
PY
}
wait_for() {
  local i=0
  until grep -q "$1" "$2" 2>/dev/null; do
    sleep 0.2; i=$((i + 1))
    [[ $i -lt $((${3:-60} * 5)) ]] || fail "timeout: '$1' in $2"
  done
}
otp_for() {
  dc logs --no-color backend 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep "dev otp" | grep "to=\"$1\"" | tail -1 \
    | sed -n 's/.*code="\([0-9]*\)".*/\1/p'
}
login() {
  cli "$1" login --email "$2" --send-only >/dev/null; sleep 1
  local c; c="$(otp_for "$2")"; [[ -n "$c" ]] || fail "no OTP in backend log"
  cli "$1" login --email "$2" --code "$c" | tee "$WORK/login-$1.json"
}

EMAIL="stack-$$@example.test"
log "login + pairing through the composed backend"
login A "$EMAIL"; grep -q '"account":"created"' "$WORK/login-A.json" || fail "A should create the account"
login B "$EMAIL"; grep -q '"account":"needs_pairing"' "$WORK/login-B.json" || fail "B should need pairing"
cli B pair offer --yes >"$WORK/pair-B.out" 2>"$WORK/pair-B.err" & BP=$!; PIDS+=("$BP")
wait_for '"event":"offer"' "$WORK/pair-B.out" 30
cli A pair join "$(jfield "$WORK/pair-B.out" offer code)" --yes >"$WORK/pair-A.out"
wait "$BP" || fail "pair offer failed: $(cat "$WORK/pair-B.err")"
grep -q '"event":"paired"' "$WORK/pair-A.out" || fail "A not paired"

round() { # label
  local d="$WORK/r-$1"; mkdir -p "$d"
  cli B clip watch --json --count 1 --timeout 90 >"$d/watch.out" 2>"$d/watch.err" & local w=$!; PIDS+=("$w")
  wait_for '"event":"ready"' "$d/watch.out" 60
  cli A clip put --text "stack $1" --wait 60 >"$d/put.out"
  wait "$w" || fail "[$1] clip watch: $(cat "$d/watch.err")"
  [[ "$(jfield "$d/watch.out" clip text)" == "stack $1" ]] || fail "[$1] clip mismatch"
  log "[$1] clip A -> B ok"
}
EXTRA=(); round direct
EXTRA=(--force-relay); round relay-only
d="$WORK/r-relay-only"; dd if=/dev/urandom of="$d/src.bin" bs=1048576 count=4 2>/dev/null
cli B fetch --out "$d/recv" --timeout 120 >"$d/fetch.out" 2>"$d/fetch.err" & F=$!; PIDS+=("$F")
wait_for '"event":"ready"' "$d/fetch.out" 60
cli A send devB "$d/src.bin" --wait 60 >"$d/send.out"
wait "$F" || fail "fetch: $(cat "$d/fetch.err")"
cmp "$d/src.bin" "$(jfield "$d/fetch.out" fetched path)" || fail "file differs"
[[ "$(jfield "$d/send.out" sent via)" == "Relay" ]] || fail "file did not go via the relay"
log "file 4 MiB via Relay ok"

log "postgres backup + restore"
export COMPOSE_PROJECT_NAME="$PROJECT" COMPOSE_ENV_FILES="$ENVF" COMPOSE_FILE="$DEPLOY/compose.yml"
STACK_DIR="$DEPLOY" BACKUP_DIR="$WORK/backups" "$DEPLOY/scripts/backup-postgres.sh"
dump="$(ls "$WORK"/backups/fsync-*.dump | head -1)"
dc exec -T postgres sh -c 'pg_restore -U "$POSTGRES_USER" -d "$POSTGRES_DB" --clean --if-exists' <"$dump" \
  || fail "pg_restore"
curl -fsS "$API/health" >/dev/null || fail "backend unhealthy after restore"
log "PASS"
