# Codebase Summary

Last updated: 2026-10-14. Repo map for the P0 Rust track. Architecture is in [system-architecture.md](./system-architecture.md); standards in [code-standards.md](./code-standards.md).

## Repo Map

| Path | Purpose |
|------|---------|
| `core/fsync-proto` | Wire types and validation: device list, pairing frames, clip/file frames, API DTOs, presence, names, caps |
| `core/fsync-core` | Client engine: identity, trust, transport (iroh), pairing, clip and file sync, encrypted history, `FsyncService` facade |
| `core/fsync-ffi` | UniFFI 0.29.4 bindings over the facade (`FsyncEngine`, contract v1) |
| `core/fsync-cli` | `fsync-cli` binary: login, pair, devices, clip, send, fetch, history, bench |
| `backend` | `fsync-backend` (axum 0.8, sqlx 0.8 Postgres): OTP auth, device-list registry, pairing codes, presence WebSocket |
| `backend/migrations` | `0001_init`, `0002_refresh_session` |
| `backend/tests` | Integration tests against Postgres via testcontainers |
| `apps/macos/Packages/FsyncCoreKit` | SwiftPM package: `FsyncCoreKit` Swift wrapper and XCTests over the xcframework |
| `apps/windows` | Empty. Windows app not started (phase 13-14) |
| `scripts` | `gen-bindings.sh` (Swift bindings + headers), `build-xcframework.sh` (universal static lib, xcframework) |
| `tests/e2e` | `p0-exit-demo.sh`: end-to-end demo (backend, CLI, relay in Docker Postgres) |
| `spikes/iroh-discovery` | Standalone spike: discovery, relay (`relay [port]`), blob measurements. Not in the workspace |
| `deploy/backend` | Backend `Dockerfile`, `compose.yml`, `.env.example` |
| `deploy/relay` | Empty. Relay deployment is phase 10 |
| `docs` | Project documentation (this folder). ADRs in `docs/adr/` |
| `plans` | Implementation plans and reports (`plans/reports/`) |
| `.github/workflows/ci.yml` | Rust job (fmt, clippy `-D warnings`, test) on Ubuntu, macOS, Windows; app jobs are placeholders |

Generated and local (not source): `target/` (Cargo), `build/` (script output: `ffi-headers`, `universal`), `*.xcframework`, `.env`.

## Crate Layout

### core/fsync-proto
`api_dto`, `clip`, `device_list`, `error`, `file`, `fixed_bytes`, `frame` (max frame 1 MiB + 256 KiB), `names`, `node_id`, `pairing`, `presence`, `shape`, `sig`, `verify`.

### core/fsync-core
| Module | Purpose |
|--------|---------|
| `api` | HTTP client (HTTPS only unless dev flag), presence session, WS utilities |
| `blobs` | Blob store trait, plain FS store, `EncryptedBlobStore` (FSB1), export |
| `clip` | Clip offers, echo guard, rate limit |
| `config` | `EngineConfig` |
| `engine` | Engine runtime and events |
| `file` | File transfer (resume, ranges, locks, policy) |
| `history` | SQLCipher history DB, keycheck, backups, retention |
| `identity` | Node identity keys |
| `net` | iroh endpoint, relay URL and path handling |
| `pair` | Pairing windows, SPAKE2/HKDF initiator and responder steps |
| `service` | `FsyncService` facade (login, pairing, devices, clips, transfers, history, presence) |
| `settings` | User settings and validation |
| `sync` | Sync coordination |
| `trust` | Device-list chain pinning, trust store (in history DB) |

### core/fsync-ffi
`FsyncEngine` object plus free functions `contract_version()` and `reset_history(data_dir)`. Contract is in [ffi-contract.md](./ffi-contract.md).

### core/fsync-cli
Global flags `--data`, `--api`, `--relay`, `--force-relay`, `--insecure-http`, `--name`. Subcommands are listed in [README.md](../README.md).

## Tests

| Location | What it covers |
|----------|----------------|
| `core/fsync-proto/tests` | Wire format, verification, caps |
| `core/fsync-core/tests` | Pairing, trust, clip and file protocols, history, settings, service facade, security limits |
| `core/fsync-ffi/tests` | Error mapping round-trip |
| `core/fsync-cli/tests` | CLI surface (help, required flags, exit codes) |
| `backend/tests` | Auth, refresh, devices, reset, presence, pairing, limits (Postgres testcontainers) |
| `apps/macos/Packages/FsyncCoreKit/Tests` | Swift XCTests (6 functions) |
| `tests/e2e` | Two-device demo over direct LAN and forced relay |

Current pass/fail state is tracked in [project-changelog.md](./project-changelog.md#verification-status), not here.

## Size Notes

Repomix compaction of the repo (262 files, include-filtered to skip build outputs) was generated for this summary in the session scratchpad, not at the repo root. It is not committed.
