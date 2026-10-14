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
| `scripts/` | `deploy.sh`, `smoke-check.sh`, `backup-postgres.sh`, `gen-env.sh`, `bootstrap-vps.sh` (fresh dedicated VPS only) |
| `backend/compose.yml` | older backend + Postgres only compose for development |

## Host Caddy snippet (owner applies; the deploy never edits or reloads Caddy)
Append to `/etc/caddy/Caddyfile`, then `sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy`:
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
`api-fsync` may stay proxied (orange cloud); WebSocket idle timeouts apply (presence heartbeat is
60 s, fine). **`relay-fsync` must be DNS-only (grey cloud)**: the relay carries long-lived upgraded
connections and, with QAD, UDP, neither of which the Cloudflare proxy supports. Both point to the
Lightsail static IP.


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
deploy/scripts/deploy.sh ubuntu@cms-node --env-file /tmp/fsync.env                 # add --public-smoke after the Caddy snippet
```
`deploy.sh` builds `linux/amd64` images locally (buildx; never compiles Rust on the VPS), ships
only the tags the host lacks (`docker save | ssh docker load`), copies compose files to `/opt/fsync`,
records `FSYNC_TAG` in the remote `.env` (600; uploaded only if missing, never overwritten), runs
`docker compose up -d`, waits for container health and smoke-checks loopback ports over SSH. Docker is
used through `sudo -n` when the ssh user is not in the docker group. Re-running the same commit
converges without rebuilding. Releases are immutable git-sha tags; the host keeps current + previous.
`--remote-build` builds on the VPS (will OOM on a 2 GiB host: avoid).

On a **dedicated** fresh VPS `scripts/bootstrap-vps.sh` installs Docker, ufw rules, swap and a backup
cron. Do not run it on the shared host.

## Rollback / stop
```bash
deploy/scripts/deploy.sh ubuntu@cms-node --rollback      # previous release (FSYNC_TAG_PREV)
ssh ubuntu@cms-node 'cd /opt/fsync && sudo docker compose -p fsync down'      # stop (data kept)
ssh ubuntu@cms-node 'cd /opt/fsync && sudo docker compose -p fsync down -v'   # stop and delete data
```
Migrations must stay backward compatible (expand/contract) for rollback to be safe.

## Staging until an email provider exists
No email sender is wired, so a `production` deployment cannot deliver OTPs. For a private pre-launch
set `FSYNC_ENV=staging`, `FSYNC_DEV_MODE=1`, `RUST_LOG=fsync_backend=debug,info` and read the code with
`docker compose logs backend | grep "dev otp"`. Never do this with real users.

## Operations
```bash
cd /opt/fsync                                  # prefix docker with sudo if needed
docker compose ps; docker compose logs -f --tail 100 backend relay
./scripts/backup-postgres.sh                   # dumps to backups/, keeps 14 days
docker compose exec -T postgres pg_restore -U fsync -d fsync --clean --if-exists < backups/<file>.dump
# daily backup (owner decides where cron lives): 15 3 * * * cd /opt/fsync && ./scripts/backup-postgres.sh
```
Backups share the disk: also enable Lightsail automatic snapshots and copy dumps off-box. Rotating
`JWT_SIGNING_KEY` signs everyone out (edit `.env`, `docker compose up -d backend`).

### Enabling QUIC address discovery
Needs the relay certificate that host Caddy manages. Set in `/opt/fsync/.env`:
`RELAY_QAD=1`, `RELAY_QUIC_BIND=0.0.0.0`, `RELAY_CERT_VOLUME=/var/lib/caddy/.local/share/caddy`
(read-only mount of Caddy's data dir; confirm the path with `sudo caddy environ | grep -i data`),
open udp/7842, then `docker compose up -d relay`. Reads a root-only directory: the owner's call.

## Local verification
`tests/e2e/compose-stack-sync.sh` builds and runs the stack with Docker (own project and free ports),
smoke-checks over loopback and over HTTPS through Caddy (local CA, relay QAD cert from Caddy), syncs a
clip and a file between two `fsync-cli` devices through the composed backend and relay
(`--force-relay`), runs a Postgres backup + restore, then `compose down -v`.
