# fsync-native

Cross-device clipboard and file sync for macOS and Windows. The P0 Rust track is the shared core (`core/`), a Postgres-backed backend (`backend/`), a UniFFI binding for Swift (`core/fsync-ffi`), and a headless CLI (`core/fsync-cli`).

Documentation lives in [docs/](./docs/). Start with [codebase-summary.md](./docs/codebase-summary.md) for the repo map and [system-architecture.md](./docs/system-architecture.md) for design.

## Prerequisites

| Tool | Needed for |
|------|-----------|
| Rust 1.99.0 (`rust-toolchain.toml`; `rustup` installs it with rustfmt and clippy) | All Rust builds and tests |
| Xcode command line tools (macOS arm64 or x86_64) | `scripts/build-xcframework.sh`, `swift test` |
| Docker | Backend tests (Postgres via testcontainers) and the e2e demo |
| Python 3 | `tests/e2e/p0-exit-demo.sh` |

## Build and Test

Run from the repo root.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Swift bindings and the macOS xcframework (writes `build/` and `apps/macos/Packages/FsyncCoreKit/FsyncFFI.xcframework`, both untracked):

```bash
scripts/build-xcframework.sh
cd apps/macos/Packages/FsyncCoreKit && swift test
```

End-to-end demo (builds backend, CLI, and the spike relay; starts Postgres 17 in Docker; runs a direct LAN round and a forced-relay round):

```bash
tests/e2e/p0-exit-demo.sh
```

| Env var | Default | Effect |
|---------|---------|--------|
| `MIB` | `256` | Size of the transferred test file |
| `CLIP_MAX_S` | `5` | Maximum allowed clip latency in seconds |
| `KEEP` | unset | `1` keeps the work dir (logs, files) after a passing run. Logs are always kept on failure |
| `CARGO_TARGET_DIR` | `<repo>/target` | Cargo output dir (made absolute by the script) |

## Run the Backend

Local (dev mode, no JWT key needed):

```bash
DATABASE_URL=postgres://USER:PASS@localhost:5432/DB FSYNC_DEV_MODE=1 cargo run -p fsync-backend
```

Docker Compose (Postgres 17 plus backend):

```bash
cp deploy/backend/.env.example deploy/backend/.env   # then edit; never commit .env
docker compose -f deploy/backend/compose.yml --env-file deploy/backend/.env up --build
```

All backend settings are listed in [docs/api.md](./docs/api.md#config-env). Set `FSYNC_ENV=production` and `JWT_SIGNING_KEY` on real deployments.

## Use the CLI

`fsync-cli` needs a data directory (`--data`). It stores keys, session, history, and blobs there (created with mode 0700). Plain `http://` backends need `--insecure-http`, which is for development only.

```bash
cargo run -q -p fsync-cli -- --data ./dev-a --api http://127.0.0.1:8080 --insecure-http \
  login --email you@example.com --send-only
cargo run -q -p fsync-cli -- --data ./dev-a --api http://127.0.0.1:8080 --insecure-http \
  login --email you@example.com --code 123456
```

| Command | Purpose |
|---------|---------|
| `login --email E [--code C] [--send-only]` | Email code sign-in |
| `pair offer [--qr] [--yes] [--timeout S]` / `pair join <INPUT> [--yes]` | Pair devices (SAS confirm) |
| `devices [--json]` | List paired devices |
| `clip put --text T [--wait S]` / `clip watch [--json] [--count N] [--timeout S]` | Send or receive clipboard text |
| `send <NODE> [PATHS]... [--wait S]` / `fetch [--out D] [--count N] [--timeout S]` | Transfer files |
| `history [--json] [--query Q] [--limit N]` / `history reset --yes` | Encrypted history |
| `bench <NODE> [--mib N] [--wait S]` | Throughput test |

Global flags: `--api` (env `FSYNC_API`), `--relay` (env `FSYNC_RELAY`), `--force-relay`, `--name`.

## Repository Layout

| Path | Contents |
|------|----------|
| `core/` | Rust crates: `fsync-proto`, `fsync-core`, `fsync-ffi`, `fsync-cli` |
| `backend/` | `fsync-backend` service and migrations |
| `apps/macos/Packages/FsyncCoreKit` | Swift package over the xcframework |
| `apps/windows/` | Windows app (WinUI 3) and C# bindings |
| `spikes/iroh-discovery` | Standalone discovery/relay spike (not in the workspace) |
| `scripts/` | Binding and xcframework build scripts |
| `tests/e2e/` | End-to-end demo |
| `deploy/` | Production stack (compose, relay, Caddy snippet, scripts); see [deployment guide](./docs/deployment-guide.md) |
| `docs/` | Documentation and ADRs |
| `plans/` | Implementation plans and reports |

## Security Notes

- Never commit `.env`, identity or session files, or the history key. `.gitignore` covers `.env` and `.env.*` (except `.env.example`).
- `build/` (script output) is not in `.gitignore` yet. Add it before the first commit.
- The e2e script and the CLI use local dev settings (plain HTTP, relay in dev mode). Do not use them for production.
