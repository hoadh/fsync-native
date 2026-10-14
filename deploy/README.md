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
| `compose.local.yml` | local-test overlay: adds a Caddy with a local CA running `Caddyfile`; never on a server |
| `Caddyfile` | the two site blocks for the host Caddy (also run verbatim by the local test) |
| `backend/Dockerfile`, `relay/Dockerfile` | images (relay = `iroh-relay` 1.3.0 from crates.io, pinned to Cargo.lock) |
| `relay/entrypoint.sh` | renders the relay config; with QAD waits for the Caddy certificate |
| `.env.example` | all settings, no secrets; `scripts/gen-env.sh` fills secrets |
| `scripts/` | `deploy.sh`, `smoke-check.sh`, `backup-postgres.sh`, `gen-env.sh`, `restore-postgres.sh`, `bootstrap-vps.sh` (fresh dedicated VPS only, needs `--dedicated`) |
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

## Staging until an email provider exists
No email sender is wired, so a `production` deployment cannot deliver OTPs. For a private pre-launch
set `FSYNC_ENV=staging`, `FSYNC_DEV_MODE=1`, `RUST_LOG=fsync_backend=debug,info` and read the code with
`docker compose logs backend | grep "dev otp"`. Never do this with real users.

## Operations
```bash
cd /opt/fsync                                  # prefix docker with sudo if needed
docker compose ps; docker compose logs -f --tail 100 backend relay
./scripts/backup-postgres.sh                   # dumps to backups/, keeps 14 days
./scripts/restore-postgres.sh backups/<file>.dump   # user/db from the stack .env; stop backend first
# deploy.sh --install-backup-cron schedules the daily backup; the script refuses below 1 GiB free
```
Backups share the disk: also enable Lightsail automatic snapshots and copy dumps off-box. Rotating
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
