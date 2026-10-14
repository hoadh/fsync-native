# Round 2 review: infra/phase-10 (HEAD 3bd677a), code-reviewer 261014-2221

Score: 8.8/10 (round 1: 7.5). Linters: actionlint clean; shellcheck (style) only info SC2016/SC2029/SC2012. `compose config` with dummy env renders TRUSTED_PROXIES=172.29.0.0/24,127.0.0.0/8, pinned subnet, user 10001:10001. Host cms-node read-only checks: stack healthy, fsync_default=172.29.0.0/24 (no overlap with 172.17/16, 172.18/16, 172.26.0.0/20; route br-5c336a9f1264 only for it), backend env shows TRUSTED_PROXIES/hops, /etc/cron.d/fsync-backup valid (user ubuntu, no `.` in name, trailing newline), only that cron file added.

## Round-1 verification
- H1 FIXED: bootstrap-vps.sh:21 requires --dedicated; remote guards :28-34 abort on foreign containers or 80/443 listener (would abort on cms-node). Note guard runs `docker ps` without sudo: if user is not in docker group `2>/dev/null` hides the failure and `others=0` (fail-open). Low, since :32-34 still catches Caddy on 443.
- H2 FIXED: compose.yml:58,106 pinned subnet, nested default verified by `config`; smoke-check.sh:38-52 behavioral probe (14 distinct XFF > per-IP 10) wired in deploy.sh:148 and e2e:66. Logic matches client_ip.rs (untrusted peer => XFF ignored => shared bucket => fails). Cloudflare/DNS-only docs (README:40-46, .env.example:25-27) correct (hops=2 alternative documented).
- M1 FIXED: compose.yml:88 mounts only RELAY_CERT_DIR (default empty named volume). QAD default off.
- M2 FIXED: relay user 10001 by default, Dockerfile USER, tmpfs /tmp for rendered TOML; RELAY_USER=0 only for QAD. Local overlay + e2e set RELAY_USER=0 (e2e:52).
- M3 FIXED: --install-backup-cron (deploy.sh:136-139), warning when not scheduled (:141), df guard (backup:17-18), restore uses container env (restore:13). cron installed on host.
- M4 FIXED: flock (deploy.sh:95-96), atomic .env via grep/.env.new/mv (:106-113); the grep rewrite also fixes the missing-trailing-newline case. Host shows .deploy.lock exists.
- M5 FIXED: compose.yml.prev swap on rollback (:99-100,102). Verified on host that .prev exists. Migrations still forward-only (documented).
- M6 FIXED: changes job + path gating, buildx gha cache with per-image scope, SHA-pinned actions. See N2/N3.
- M7 FIXED: max_connections=25 (backend pool 20, db.rs:9), memswap_limit equal to mem_limit on all three.
- Lows: .dockerignore added (root context; workspace members are core/* + backend only, Dockerfile copies only those, so excluded dirs are safe), COPYFILE_DISABLE, -m 3 on WS probe, umask 077 in backup, SHA pins. L9 --remote-build removed.

## Critical / High
None.

## Medium
N1 deploy.sh:81 vs :95-96 - `rm -rf .incoming && mkdir` and tar extraction happen BEFORE the flock is taken. A second concurrent deploy wipes/overwrites the first deploy's staging dir mid-run (first then `cp .incoming/compose.yml` fails under set -e, after .env possibly partly switched? No: .env swap is after the cp, so failure is early, but compose.yml may already have been replaced by the other deploy's version). Also image `docker load` is outside the lock (harmless). Fix: take the lock in a separate earlier ssh is not possible across calls; simplest is to stage into a unique dir (`.incoming.$$`/`mktemp -d`) and pass it to the REMOTE script. Low likelihood (single operator) so Medium-Low.
N2 ci.yml:41-42 - path filters miss inputs: macos job does not trigger on `scripts/gen-bindings.sh` (called by build-xcframework.sh:21); windows job does not trigger on `scripts/gen-bindings-cs.ps1` (called by build-windows-dll.ps1:29). Edits to those would skip the platform tests (skip reports as success on required checks). Fix: use `^scripts/(build-xcframework\.sh|gen-bindings\.sh)$` and `^scripts/(build-windows-dll|gen-bindings-cs)\.ps1$` (or just `^scripts/`).

## Low
N3 ci.yml:33-36 `changes` logic otherwise sound: fetch-depth 0, new-branch/force-push (`before` zeros or missing) and diff failure fall back to ALL, dispatch forces ALL, rust job unconditional, `needs: changes` skips only when changes succeeded with false. Residual: on main, a push containing only unrelated paths skips mac/win by design; if mac/win are required checks, a failure of `changes` itself still fails the workflow, so no silent skip. No action, just document that skip == pass.
N4 compose.yml:22,24 - cms-node has 1.9 GiB RAM, ~104 MiB free + 1 GiB of 2 GiB swap already used by other tenants. memswap_limit=limit means our containers OOM-restart instead of swapping (intended); Postgres at 192m with shared_buffers 64MB + 25 conns is fine, but expect OOM restarts of backend (96m) under load; monitor `docker inspect ... OOMKilled`.
N5 backup-postgres.sh:23 pg_dump needs one connection; with backend pool 20 + max_connections 25 (3 reserved) = 22 usable, leaves 2 for dump/healthcheck/restore. Tight, fine; raising max_connections to 30 costs little.
N6 Host has stale macOS `._Caddyfile ._compose.yml ._scripts` in /opt/fsync from pre-fix deploys (harmless; `rm` them manually; `._scripts` has +x). Not a repo issue.
N7 bootstrap-vps.sh:29 guard fail-open when docker needs sudo (see H1 note): use `sudo`-less run since script already runs under sudo (it does: `sudo ... bash -s`), so actually fine; only the `ps -a` with missing docker daemon case. Informational.
N8 deploy.sh:102-104 files copied with umask 077: compose.yml/Caddyfile 600 (Caddyfile is 644 on host from earlier). Host Caddy reads the snippet only if it `import`s /opt/fsync/Caddyfile and runs as caddy user: 600 would break reload after next deploy (existing 644 stays since cp onto existing keeps mode, but a fresh dir would not). Verify Caddy import path/perms, or `chmod 644 Caddyfile` in the REMOTE block.

## Shared-host safety (re-checked)
Writes limited to /opt/fsync, docker project fsync (images fsync-*, network fsync_default, volumes fsync_*), and /etc/cron.d/fsync-backup (opt-in, own file). No ufw/swap/daemon/Caddy edits; prune scoped to fsync repos; `--remove-orphans` project-scoped; host ports 18080/18081/7842 on loopback do not collide with listeners seen (8080/8081 are 0.0.0.0 by other tenants). Smoke probe opens 14 short-lived unauth sockets on backend loopback only.

## Secrets
None committed; .env 600, uploaded via umask 077 + atomic mv; CI has no secrets/VPS creds; actions SHA-pinned with contents: read.

## Unresolved
- Caddyfile perms/import path on host (N8).
- Are mac/win jobs required checks? (affects N2 impact)

**Status:** DONE_WITH_CONCERNS
**Summary:** All round-1 findings fixed and verified in code and on cms-node; no critical/high issues. 8.8/10.
**Concerns/Blockers:** N1 (staging dir race before flock), N2 (CI path filters miss gen-bindings scripts), N8 (Caddyfile mode 600 under umask 077 on fresh dirs). Non-blocking.
