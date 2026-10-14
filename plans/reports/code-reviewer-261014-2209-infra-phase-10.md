# Review: infra/phase-10 vs main (code-reviewer, 261014-2209)

Score: 7.5/10. Linters: actionlint clean; shellcheck -S warning clean (only info-level SC2016/SC2029/SC2012). No secrets committed (.env ignored, .env.example empty secrets). All new files < 200 lines.

## Critical
None.

## High
H1 deploy/scripts/bootstrap-vps.sh:24-30,37-41 - Only "do not run on shared host" in README:70; no code guard. On cms-node it would `ufw --force enable` (firewall presumably inactive per README:46; enabling with only 22/80/443/7842 allowed blocks anything else the host serves, and 22 is hard-coded so a non-default sshd port locks out), add swap, write cron.d, add user to docker group. Fix: refuse unless `--i-know-dedicated` flag, or abort when ufw inactive/docker already installed.
H2 deploy/compose.yml:54 + README (TRUSTED_PROXIES=172.16.0.0/12) - Caddy runs on the host and hits the published loopback port; the backend sees the docker bridge gateway (172.x.0.1, via docker-proxy). Shared host with many compose projects: docker's default pool moves past 172.31 into 192.168.x.0/20 (or daemon default-address-pools may be 10.x), and the new `fsync_default` network may land outside 172.16/12. client_ip.rs:27-35: untrusted peer => XFF ignored => every user shares one rate-limit bucket (MAX_UNAUTH_WS_PER_IP=10 total) => self-DoS of unauth WS. Not caught by CI (e2e uses plain loopback, not XFF). Fix: pin the network in compose (`networks: default: ipam: config: - subnet: 172.29.250.0/24`) and set TRUSTED_PROXIES to it; add a deploy smoke assertion (e.g. backend log/health shows real client IP) or at least document the check. Also verify with userland-proxy disabled the source may be 127.0.0.1; consider adding 127.0.0.0/8.

## Medium
M1 deploy/compose.yml:83 + README:98-101 + relay/entrypoint.sh:24 - RELAY_QAD=1 mounts Caddy's whole data dir (ACME account key + private keys of ALL host sites: wordpress/moodle) into a root container. Mount only `certificates/<ca>/<relay-host>/` (bind that subdir), or copy cert+key to a dedicated dir via a Caddy hook/cron. Default off, so not blocking.
M2 deploy/compose.yml:84 - relay `user: "0"` even when QAD off. Make root conditional (override only in QAD compose snippet) or document.
M3 deploy/scripts/backup-postgres.sh + deploy.sh - on the shared host no backup runs: cron is installed only by bootstrap (which must not be run); README:92 leaves it to the owner. Add a documented/installed-by-deploy step or an explicit "backups are NOT scheduled" warning in deploy output. Dumps are same-disk only (README:94 acknowledges). Also no free-disk check; restore command hardcodes -U fsync -d fsync (README:91, script:7) while env allows overrides.
M4 deploy/scripts/deploy.sh:96-121 - no remote lock (flock) against concurrent deploys; `sed -i` edits of .env are non-atomic and `echo >> .env` corrupts the last line if the uploaded .env lacks a trailing newline (gen-env output has one, hand-written may not). Use `flock -n /opt/fsync/.deploy.lock` and `[ -z "$(tail -c1 .env)" ] || echo >> .env` first.
M5 Rollback: deploy.sh:49-52,79-80 correct for images (FSYNC_TAG/PREV swap toggles properly; failed deploy keeps PREV; retry of same tag keeps PREV). Caveats: rollback copies the *current checkout's* compose.yml (deploy.sh:87), so compose/env changes are not rolled back; DB migrations not rolled back (README:79 notes). Postgres container gets recreated if compose.yml changes; confirm volume `pgdata` is project-scoped (it is: fsync_pgdata).
M6 CI cost: ci.yml:36-82 runs macos-15 (10x minutes) and windows (2x) on every PR push, plus deploy-stack.yml also runs a full uncached Docker Rust build twice on every PR touching core/backend (e2e builds backend+relay images; rust-cache does not cache Docker builds). Triggers themselves are correct (PR + main + dispatch; no all-branch push; cancel-in-progress). Suggest path filters for macos/windows jobs (apps/**, core/**, scripts/**) and buildx GHA cache (`cache-from/to type=gha`) or Docker layer cache for deploy-stack; relay image rebuilds `cargo install iroh-relay` from scratch each time (deploy/relay/Dockerfile:19).
M7 Memory: compose.yml:24,62,85 = 192+96+96 = 384 MiB of a 1.9 GiB host shared with mysql/moodle/wordpress. Postgres: shared_buffers 64MB + max_connections=40 x work_mem could exceed 192m under load => OOM-kill/restart loops (restart: unless-stopped). No memswap_limit so containers can also swap (x2). Check backend pool size <= ~15 and consider max_connections=20. No pids_limit/cpus.

## Low
L1 No .dockerignore at repo root: build context ships the whole repo incl. `target/` (multi-GB) on every local `buildx build` (deploy.sh:73-75). Add one (target, .git, plans, apps).
L2 deploy.sh:87 tar from macOS adds `._*`/xattr headers (GNU tar warnings); set `COPYFILE_DISABLE=1`. Extraction only touches compose.yml, Caddyfile, scripts/ (safe; remote Caddy is never edited, matches README).
L3 deploy.sh:119 prune only touches `fsync-backend`/`fsync-relay` repos and `|| true`; safe. No `docker system/volume/network prune`, no global `down`. `--remove-orphans` is scoped to project `fsync` (name pinned in compose.yml:8) - a different compose project named `fsync` or COMPOSE_PROJECT_NAME in .env would change scope; fine.
L4 deploy.sh:46 `install -d ... -m 750` ok; `$dir` interpolated unescaped into remote shell (operator-supplied only).
L5 smoke-check.sh:23-26 WebSocket probe: curl hangs on 101 until --max-time 10 (adds 10 s, result still correct via `|| true`). Add `-m 3`.
L6 Actions pinned by tag, not SHA (checkout@v4, rust-cache@v2, cache@v4, dtolnay@1.99.0). Acceptable for private repo; permissions: contents: read is set (good).
L7 backup-postgres.sh: umask 022 dump files, dir 700 so fine; retention `-mtime +14` scoped to fsync-*.dump; magic-byte check + partial-file trap good. Add `umask 077`.
L8 relay HEALTHCHECK/entrypoint: entrypoint heredoc interpolates env into TOML unvalidated (trusted input only).
L9 deploy.sh:43-45 mixes `sudo -n docker`; with `--remote-build` DOCKER_HOST=ssh:// fails if user lacks docker group (and OOMs the host - README warns). Consider removing the option (YAGNI).

## Verified OK
- Postgres: no `ports:`, only compose network. Backend/relay published on 127.0.0.1 only; QUIC bind defaults to loopback; metrics 9090 unpublished. Note Docker bypasses ufw for published ports, so the loopback defaults are what protect (good).
- Caddy snippet: flush_interval -1 on relay; websockets proxied by default; Caddy overwrites untrusted XFF so TRUST_PROXY_HOPS=1 is right for exactly one proxy. Cloudflare in front of api would add a second hop (README:40 says api may stay proxied => XFF has 2 entries; hops=1 then picks the Cloudflare IP => all users bucketed by CF edge). Fix: document hops=2 when api is orange-cloud, or advise grey cloud. (Treat as medium if that DNS mode is actually used.)
- Secrets: gen-env.sh uses openssl, writes stdout only; .env uploaded with umask 077 and chmod 600; not echoed. CI uses no secrets; deploy-stack.yml never touches the VPS (no credentials). `compose config -q` in CI uses a generated throwaway env.
- ci.yml: no Postgres service needed (testcontainers, backend/tests/common/pg.rs; `docker pull postgres:17` prewarm good). Toolchain 1.99.0 matches rust-toolchain.toml. xcframework and windows scripts add their own rustup targets. Windows NASM/Perl PATH step and uniffi-bindgen-cs cache look right (cache save order is safe). macos-15 runner valid.
- deploy.sh: set -euo pipefail, quoting adequate, idempotent re-run, health wait + log tail on failure, never overwrites remote .env.

## Unresolved
- Which docker address pool/subnet does cms-node use for new networks (`docker network ls`, `docker info`)? Confirms H2.
- Is api-fsync proxied by Cloudflare (hops)?
- Who schedules backups on cms-node?

Status: DONE_WITH_CONCERNS
