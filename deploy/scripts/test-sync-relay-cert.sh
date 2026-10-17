#!/usr/bin/env bash
# Local check of sync-relay-cert.sh with Docker. A fake Caddy store (0600, other owner) holds four
# issuer dirs for one host: valid (40 d), newest valid (60 d), near expiry (1 d) and a mismatched
# pair (90 d), plus a 10 d cert for another host. The script, run as root, must pick the 60 d pair, set root:10001 0550/0440, stay
# silent when nothing changed, refuse bad input, and exit 3 near expiry. Then the relay image runs with RELAY_QAD=1 as
# its normal uid 10001 on the copy and must bind udp/7842.
#   deploy/scripts/test-sync-relay-cert.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
H=relay.localhost IMG=fsync-relay:certsync-test NAME=fsync-certsync-test
# A named volume, not a bind mount: root owner/mode semantics as on the Linux host (Docker Desktop
# bind mounts do not let container root bypass permissions).
VOL=fsync-certsync-test
cleanup() { docker rm -f "$NAME" >/dev/null 2>&1 || true; docker volume rm -f "$VOL" >/dev/null 2>&1 || true; }
trap cleanup EXIT
fail() { printf 'FAIL %s\n' "$*" >&2; docker logs --tail 30 "$NAME" >&2 2>/dev/null || true; exit 1; }

cleanup
# Ubuntu-like tools on the host: GNU date (coreutils), openssl.
in_alpine() { docker run --rm -v "$VOL:/w" -v "$HERE/scripts:/s:ro" alpine:3.20 sh -c "apk add -q openssl coreutils >/dev/null && $1"; }
run() { in_alpine "CADDY_CERTS=/w/caddy /s/sync-relay-cert.sh $*"; }
in_alpine "set -e; mk() { d=/w/caddy/\$1/$H; mkdir -p \$d
    openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days \$2 -subj /CN=$H \
      -addext subjectAltName=DNS:$H -keyout \$d/$H.key -out \$d/$H.crt 2>/dev/null; }
  mk acme-v02.example-directory 40; mk zerossl.example-directory 60; mk stale.example-directory 1
  mk mismatch.example-directory 90
  openssl genpkey -algorithm ec -pkeyopt ec_paramgen_curve:P-256 -out /w/caddy/mismatch.example-directory/$H/$H.key
  d=/w/caddy/acme-v02.example-directory/soon.localhost; mkdir -p \$d
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 10 -subj /CN=soon.localhost \
    -keyout \$d/soon.localhost.key -out \$d/soon.localhost.crt 2>/dev/null
  chown -R 999:999 /w/caddy && chmod 600 /w/caddy/*/*/*"

out="$(run "$H /w/out")"
[[ "$out" == updated* ]] || fail "first run printed: $out"
in_alpine "cmp -s /w/out/$H.crt /w/caddy/zerossl.example-directory/$H/$H.crt" || fail "did not pick the newest valid, matching pair"
modes="$(in_alpine "stat -c '%a %u:%g %n' /w/out /w/out/$H.crt /w/out/$H.key; ls -A /w/out | tr '\n' ' '")"
echo "$modes"
grep -q "^550 0:10001 /w/out$" <<<"$modes" || fail "dest dir mode/owner"
grep -q "^440 0:10001 /w/out/$H.crt$" <<<"$modes" || fail "crt mode/owner"
grep -q "^440 0:10001 /w/out/$H.key$" <<<"$modes" || fail "key mode/owner"
grep -q "^$H.crt $H.key $" <<<"$(tail -1 <<<"$modes")" || fail "leftover files in dest"
in_alpine "chmod 644 /w/out/$H.key"
[[ -z "$(run "$H /w/out")" ]] || fail "second run not silent"
[[ "$(in_alpine "stat -c %a /w/out/$H.key")" == 440 ]] || fail "mode not repaired on an unchanged run"
run "nope.localhost /w/out2" 2>/dev/null && fail "missing cert accepted"
run "../etc /w/out3" 2>/dev/null && fail "bad hostname accepted"
set +e; run "soon.localhost /w/out4" >/dev/null 2>&1; rc=$?; set -e
[[ $rc == 3 ]] || fail "near-expiry copy: exit $rc, want 3"

# The relay as uid 10001 (no RELAY_USER=0) must read the copy and bind QUIC.
docker build -q -t "$IMG" "$HERE/relay" >/dev/null
docker run -d --name "$NAME" --user 10001:10001 --read-only --tmpfs /tmp --cap-drop ALL \
  -e RELAY_HOSTNAME=$H -e RELAY_QAD=1 -e RUST_LOG=info --mount "type=volume,src=$VOL,dst=/relay-cert,volume-subpath=out,readonly" "$IMG" >/dev/null
for _ in $(seq 1 30); do
  docker exec "$NAME" sh -c 'cat /proc/net/udp /proc/net/udp6 2>/dev/null' | grep -qi ':1EA2 ' && break
  sleep 1
done
[[ "$(docker inspect -f '{{.State.Running}}' "$NAME")" == true ]] || fail "relay exited"
docker exec "$NAME" sh -c 'cat /proc/net/udp /proc/net/udp6 2>/dev/null' | grep -qi ':1EA2 ' || fail "no udp/7842 listener"
docker logs "$NAME" 2>&1 | grep -q "QUIC server listening" || fail "no QUIC server log line"
echo "PASS sync-relay-cert: newest valid pair, root:10001 0550/0440, idempotent, relay QAD up without root"
