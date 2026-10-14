# Deploying fsync (single VPS)

One Linux VPS (AWS Lightsail) runs backend + iroh relay + Postgres with Docker Compose. TLS is
terminated by a Caddy that is **not** in the compose file (the VPS already runs a host Caddy on
80/443); the stack only listens on 127.0.0.1.

```
internet --443--> host Caddy (ACME) --> 127.0.0.1:18080 backend   api-fsync.10z.one  (REST + presence WebSocket)
                                    --> 127.0.0.1:18081 relay     relay-fsync.10z.one (iroh relay, HTTP/WebSocket)
internet --7842/udp--> relay (QUIC address discovery; optional, off by default)
                       backend --> postgres (compose network only)
```

| File | What |
|---|---|
| `compose.yml` | the stack (project `fsync`); memory limits sized for a ~2 GiB shared host |
| `compose.relay-only.yml`, `relay-only.env.example` | relay alone (no backend, no Postgres) for the Singapore host, see "Relay-only host" |
| `compose.local.yml` | local-test overlay: adds a Caddy with a local CA running `Caddyfile`; never on a server |
| `Caddyfile` | the two site blocks for the host Caddy (also run verbatim by the local test) |
| `backend/Dockerfile`, `relay/Dockerfile` | images (relay = `iroh-relay` 1.3.0 from crates.io, pinned to Cargo.lock) |
| `relay/entrypoint.sh` | renders the relay config; with QAD waits for the Caddy certificate |
| `.env.example` | all settings, no secrets; `scripts/gen-env.sh` fills secrets |
| `scripts/` | `deploy.sh`, `smoke-check.sh`, `backup-postgres.sh`, `gen-env.sh`, `restore-postgres.sh`, `bootstrap-vps.sh` (fresh dedicated VPS only, needs `--dedicated`), `test-relay-only-local.sh` |
| `backend/compose.yml` | older backend + Postgres only compose for development |

## Host Caddy snippet (owner applies; the deploy never edits or reloads Caddy)
The host Caddy does **not** read `/opt/fsync/Caddyfile` (that copy is only for reference and the local test);
the owner pastes the blocks below into `/etc/caddy/Caddyfile`, then `sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy`:
```
api-fsync.10z.one {
	reverse_proxy 127.0.0.1:18080
}

relay-fsync.10z.one {
	reverse_proxy 127.0.0.1:18081 {
		flush_interval -1
	}
}
```

## DNS
Both hostnames point to the Lightsail static IP and should be **DNS-only (grey cloud)**.
`relay-fsync` must be: the relay carries long-lived upgraded connections and, with QAD, UDP, neither
of which the Cloudflare proxy supports. `api-fsync` should be too: host Caddy has no
`trusted_proxies`, so behind Cloudflare it would replace `X-Forwarded-For` with the Cloudflare edge
IP and every per-IP limit (unauthenticated presence sockets, rate limits) would bucket by edge.
(Alternative, not recommended: keep it orange, add Caddy global `servers { trusted_proxies static
<Cloudflare ranges> }` so XFF keeps the real client, and set `TRUST_PROXY_HOPS=2`.)

## Client IP trust
Caddy connects to the backend's published loopback port, so the backend sees the gateway of the compose
network (docker-proxy) or 127.0.0.1 (userland proxy off). The network is pinned to `FSYNC_SUBNET`
(default `172.29.0.0/24`; must not overlap other docker networks or the VPC: on the current host 172.17-19/16
and 172.26.0.0/20 are in use) and `TRUSTED_PROXIES` defaults to that subnet + `127.0.0.0/8`.
`smoke-check.sh` with `CHECK_PROXY_TRUST=1` (run by `deploy.sh` on the host and by the e2e test)
opens more unauthenticated presence sockets than the per-IP limit, each with a different
`X-Forwarded-For`; it passes only if the backend honors the header from its peer.

## Ports (Lightsail firewall; ufw is inactive on the shared host and we do not touch it)
| Port | Proto | Needed | Why |
|---|---|---|---|
| 443 | tcp | already open | host Caddy: api + relay HTTPS/WebSocket |
| 80 | tcp | already open | ACME + redirect |
| 7842 | udp | only with `RELAY_QAD=1` | relay QUIC address discovery (NAT traversal). 443/udp is taken by Caddy |
| 22 | tcp | already open | SSH |

Nothing else is published beyond loopback (backend 18080, relay 18081; Postgres and relay metrics
9090 stay inside the compose network).

## Deploy
```bash
deploy/scripts/gen-env.sh api-fsync.10z.one relay-fsync.10z.one > /tmp/fsync.env   # secrets stay out of git
deploy/scripts/deploy.sh <ssh-target> --env-file /tmp/fsync.env                 # add --public-smoke after the Caddy snippet
```
`deploy.sh` builds `linux/amd64` images locally (buildx; never compiles Rust on the VPS), ships
only the tags the host lacks (`docker save | ssh docker load`), copies compose files to `/opt/fsync`,
records `FSYNC_TAG` in the remote `.env` (600; uploaded only if missing, never overwritten), runs
`docker compose up -d`, waits for container health and smoke-checks loopback ports over SSH. Docker is
used through `sudo -n` when the ssh user is not in the docker group. Re-running the same commit
converges without rebuilding. Releases are immutable git-sha tags; the host keeps current + previous.
Concurrent deploys are refused (flock), `.env` is rewritten atomically, and `compose.yml.prev` is kept.
`--install-backup-cron` writes `/etc/cron.d/fsync-backup` (daily 03:15, only that file).

On a **dedicated** fresh VPS `scripts/bootstrap-vps.sh HOST --dedicated` (aborts when other containers or
something on 80/443 exist) installs Docker, ufw rules, swap and a backup
cron. Do not run it on the shared host.

## Rollback / stop
```bash
deploy/scripts/deploy.sh <ssh-target> --rollback      # previous release (FSYNC_TAG_PREV)
ssh <ssh-target> 'cd /opt/fsync && sudo docker compose -p fsync down'      # stop (data kept)
ssh <ssh-target> 'cd /opt/fsync && sudo docker compose -p fsync down -v'   # stop and delete data
```
Rollback restores the previous images **and** the previous `compose.yml`. Migrations are forward-only (never
rolled back): keep them backward compatible (expand/contract) so the old release still runs on the new schema.
`--rollback` refuses (`scripts/rollback-guard.sh`) a target release that does not contain every migration the
database already applied: sqlx would crash-loop it. `--force-rollback` skips the check. A second `--rollback`
rolls forward again (the two tags swap). Rehearsal evidence: [operations.md](operations.md).

## OTP email (Gmail SMTP)
The backend sends sign-in codes with `EMAIL_SENDER=smtp` (STARTTLS on `smtp.gmail.com:587`, certificate
verified, no plaintext fallback). Variables, all in `/opt/fsync/.env` (mode 600, **never in the repo**):
`EMAIL_SENDER=smtp`, `SMTP_USERNAME=<gmail address>`, `SMTP_PASSWORD=<app password>`,
`SMTP_FROM="fsync <gmail address>"`; `SMTP_HOST`/`SMTP_PORT` default to Gmail and 587.

Create the App Password (Google refuses the normal password for SMTP):
1. Sign in to the sender Google account and turn on **2-Step Verification** (Security > How you sign in to Google). App Passwords are not offered without it.
2. Open <https://myaccount.google.com/apppasswords>, name it `fsync`, click Create and copy the 16 characters (shown in groups of four; the spaces are ignored).
3. On the host: `sudoedit /opt/fsync/.env`, set the four variables above and delete any `ALLOW_LOG_EMAIL=1` line.
4. `cd /opt/fsync && docker compose up -d backend`, then request a code from the app. Failures are logged as a category only (`smtp timeout`, `smtp rejected (permanent) 535`, ...), never the password or the code.

Rules: with `FSYNC_ENV=production` the backend **refuses to start** unless `EMAIL_SENDER=smtp` or
`ALLOW_LOG_EMAIL=1` (explicit: codes are not delivered). `gen-env.sh` writes `EMAIL_SENDER=smtp` with empty
`SMTP_*` values, so a new host refuses to start until they are filled. `deploy.sh` appends `ALLOW_LOG_EMAIL=1` (with a warning) to an existing host `.env` that has neither setting, so the current
staging host keeps running; removing it after configuring SMTP is the switch-over. To rotate, revoke the App
Password in the Google account. Consumer Gmail allows about **500 recipients per day** (and throttles bursts):
`OTP_DAILY_SEND_CAP` (default 450 per rolling 24 h, kept in Postgres) makes `start` return 429 before that
limit is hit. A failed send (provider outage) does not spend the per-email/per-IP quota or a global slot.
Fine for a pre-launch, use a transactional provider before real traffic (only the `SMTP_*` values change).
For private testing without any mail, `FSYNC_ENV=staging`, `FSYNC_DEV_MODE=1`,
`RUST_LOG=fsync_backend=debug,info` and `docker compose logs backend | grep "dev otp"` still work; never with real users.

## Relay-only host (Singapore)
A second relay region on a host with **no backend and no Postgres**: only the relay container
(`compose.relay-only.yml`), on loopback behind that host's own Caddy, the same pattern as the main host.
The host is shared (its Caddy already serves another site), so: do **not** run `bootstrap-vps.sh`, there is no
ACME in the container, nothing is published beyond loopback, and nothing here edits or reloads Caddy.
```
internet --443--> host Caddy (ACME) --> 127.0.0.1:18081 relay      relay-sg-fsync.10z.one
```

Hosts deployed before the `.mode` marker existed: run `echo relay-only > <dir>/.mode` once on a relay-only host (default `/opt/fsync/.mode`); otherwise a `--rollback` without `--relay-only` is not auto-detected.

**1. Docker (once, run by the owner/lead; official docker-ce apt repo + compose plugin, Ubuntu 24.04)**
```bash
ssh <ssh-target>
sudo apt-get update && sudo apt-get install -y ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
sudo apt-get update && sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
sudo docker run --rm hello-world && sudo docker compose version
```
This only adds an apt source and Docker's own iptables chains; Caddy and its sockets are unaffected. Check
`ip route` first that Docker's default bridge (172.17.0.0/16) does not overlap the VPC. The ssh user need not
join the `docker` group: `deploy.sh` falls back to `sudo -n docker`, and stops with a pointer here when Docker is
missing. The host has 1.9 GiB RAM: the relay is capped at 128 MB, and Docker itself needs ~100 MB.

**2. Deploy**
```bash
cp deploy/relay-only.env.example /tmp/relay-sg.env      # RELAY_HOST=relay-sg-fsync.10z.one, no secrets
deploy/scripts/deploy.sh <ssh-target> --relay-only --env-file /tmp/relay-sg.env
```
Builds only the relay image (`linux/amd64`, locally), ships it, installs `compose.relay-only.yml` as
`/opt/fsync/compose.yml`, starts it and smoke-checks `127.0.0.1:18081` over SSH (`smoke-check.sh --relay-only`).
Releases and `--skip-smoke` work as on the main host; pass `--relay-only` to `--rollback` too.

**3. Host Caddy snippet** (owner pastes into `/etc/caddy/Caddyfile`, then `sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy`)
```
relay-sg-fsync.10z.one {
	reverse_proxy 127.0.0.1:18081 {
		flush_interval -1
	}
}
```
DNS: A record to the instance IP, **DNS-only (grey cloud)**. Then verify publicly:
`deploy/scripts/deploy.sh <ssh-target> --relay-only --env-file /tmp/relay-sg.env --public-smoke`
or `deploy/scripts/smoke-check.sh --relay-only https://relay-sg-fsync.10z.one`.

**Firewall (Lightsail > Networking):** nothing new by default; 80/tcp, 443/tcp and 22/tcp are already open.
QUIC address discovery stays **off**, as on the main host (nothing listens publicly on udp/7842; clients just use
the relay). To enable it, follow "Enabling QUIC address discovery" below with this host's certificate directory
(`.../certificates/acme-v02.api.letsencrypt.org-directory/relay-sg-fsync.10z.one`), `RELAY_QUIC_BIND=0.0.0.0`,
and add the Lightsail rule **UDP 7842 from anywhere**. udp/443 is Caddy's HTTP/3 and unrelated.

Local check: `deploy/scripts/test-relay-only-local.sh` (builds the image, runs the file on loopback ports,
runs `smoke-check.sh --relay-only`, asserts only the relay runs and nothing is published beyond 127.0.0.1).

## Operations
```bash
cd /opt/fsync                                  # prefix docker with sudo if needed
docker compose ps; docker compose logs -f --tail 100 backend relay
./scripts/backup-postgres.sh                   # dumps to backups/, keeps 14 days
./scripts/restore-postgres.sh backups/<file>.dump   # user/db from the stack .env; stop backend first
# deploy.sh --install-backup-cron schedules the daily backup; the script refuses below 1 GiB free
```
Off-box copies, health probe and rollback rehearsal: [operations.md](operations.md). Backups share the
disk: also enable Lightsail automatic snapshots. Rotating
`JWT_SIGNING_KEY` signs everyone out (edit `.env`, `docker compose up -d backend`).

### Enabling QUIC address discovery
Needs the relay certificate that host Caddy manages, mounted **alone** (never Caddy's whole data dir,
which holds every site's keys). Set in `/opt/fsync/.env` (path: `sudo caddy environ | grep -i data`;
the directory exists once Caddy has served the relay host):
`RELAY_QAD=1`, `RELAY_QUIC_BIND=0.0.0.0`, `RELAY_USER=0` (Caddy's key is root-only 0600),
`RELAY_CERT_DIR=/var/lib/caddy/.local/share/caddy/certificates/acme-v02.api.letsencrypt.org-directory/relay-fsync.10z.one`,
open udp/7842, then `docker compose up -d relay`. Without QAD the relay runs as uid 10001.

## Local verification
`tests/e2e/compose-stack-sync.sh` builds and runs the stack with Docker (own project and free ports),
smoke-checks over loopback and over HTTPS through Caddy (local CA, relay QAD cert from Caddy), syncs a
clip and a file between two `fsync-cli` devices through the composed backend and relay
(`--force-relay`), runs a Postgres backup + restore, then `compose down -v`.
