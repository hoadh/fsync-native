#!/usr/bin/env bash
# External health probe for the public endpoints (run hourly by .github/workflows/health-probe.yml;
# also usable by hand). Exits 1 listing every failure, so the failed run emails the repo owner.
#   deploy/scripts/probe-health.sh
# Env: API_HOST (api-fsync.10z.one), RELAY_HOSTS (space separated; relay-fsync.10z.one
#      relay-sg-fsync.10z.one), CERT_WARN_DAYS (14), RETRIES (3).
set -uo pipefail
API_HOST="${API_HOST:-api-fsync.10z.one}"
RELAY_HOSTS="${RELAY_HOSTS:-relay-fsync.10z.one relay-sg-fsync.10z.one}"
CERT_WARN_DAYS="${CERT_WARN_DAYS:-14}"
RETRIES="${RETRIES:-3}"
fails=()

# fetch URL -> sets code, body; retries so one dropped packet does not page anyone.
fetch() {
  local i
  for ((i = 1; i <= RETRIES; i++)); do
    body="$(curl -sS --max-time 10 -o - -w '\n%{http_code}' "$1" 2>/dev/null)" && break
    sleep 3
  done
  code="${body##*$'\n'}"; body="${body%$'\n'*}"
  [[ "$code" =~ ^[0-9]{3}$ ]] || code=000
}
check() { # name url expected_code expected_body_regex
  fetch "$2"
  if [[ "$code" == "$3" && "$body" =~ $4 ]]; then printf 'ok   %s\n' "$1"
  else printf 'FAIL %s: HTTP %s\n' "$1" "$code" >&2; fails+=("$1"); fi
}
cert_days() { # host -> days until the served certificate expires (empty on failure)
  local end
  end="$(echo | openssl s_client -servername "$1" -connect "$1:443" 2>/dev/null | openssl x509 -noout -enddate 2>/dev/null | cut -d= -f2)" || return 0
  [[ -n "$end" ]] || return 0
  local t
  t="$(date -d "$end" +%s 2>/dev/null || date -j -f '%b %e %T %Y %Z' "$end" +%s 2>/dev/null)" || return 0
  echo $(((t - $(date +%s)) / 86400))
}

check "api /health" "https://$API_HOST/health" 200 '^ok$'
# 503 means the global OTP send cap is reached (sign-ins refused for up to 24 h). 404 means the
# running release predates the endpoint: warn only.
fetch "https://$API_HOST/health/otp"
case "$code" in
  200) echo "ok   api otp budget";;
  404) echo "warn api /health/otp missing (release predates it)";;
  *) printf 'FAIL api otp budget: HTTP %s %s\n' "$code" "$body" >&2; fails+=("otp-budget");;
esac
for h in $RELAY_HOSTS; do check "relay $h" "https://$h/healthz" 200 'ok'; done
for h in "$API_HOST" $RELAY_HOSTS; do
  d="$(cert_days "$h")"
  if [[ -z "$d" ]]; then echo "warn cert expiry of $h unreadable"
  elif ((d < CERT_WARN_DAYS)); then printf 'FAIL cert %s expires in %s days\n' "$h" "$d" >&2; fails+=("cert $h")
  else echo "ok   cert $h ($d days)"; fi
done
if ((${#fails[@]})); then printf 'probe failed: %s\n' "${fails[*]}" >&2; exit 1; fi
