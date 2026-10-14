# Codebase Summary

Last updated: 2026-10-14. Repo map for the P0 Rust track and the macOS app shell. Architecture is in [system-architecture.md](./system-architecture.md); standards in [code-standards.md](./code-standards.md).

## Repo Map

| Path | Purpose |
|------|---------|
| `core/fsync-proto` | Wire types and validation: device list, pairing frames, clip/file frames, API DTOs, presence, names, caps |
| `core/fsync-core` | Client engine: identity, trust, transport (iroh), pairing, clip and file sync, encrypted history, `FsyncService` facade |
| `core/fsync-ffi` | UniFFI 0.29.4 bindings over the facade (`FsyncEngine`, contract v2) |
| `core/fsync-cli` | `fsync-cli` binary: login, pair, devices, clip, send, fetch, history, bench |
| `backend` | `fsync-backend` (axum 0.8, sqlx 0.8 Postgres): OTP auth, device-list registry, pairing codes, presence WebSocket |
| `backend/migrations` | `0001_init`, `0002_refresh_session` |
| `backend/tests` | Integration tests against Postgres via testcontainers |
| `apps/macos/Packages/FsyncCoreKit` | SwiftPM package: `FsyncCoreKit` Swift wrapper and XCTests over the xcframework; `TrustResetPrompt` SwiftUI library (trust-reset prompt) with its tests |
| `apps/macos/FsyncMac` | SwiftPM app (tools 6.0, macOS 13): `FsyncMacCore` (EngineBridge, Keychain, models), `FsyncMacUI` (SwiftUI views), `FsyncMac` (`@main` menubar app); tests in `FsyncMacCoreTests` (incl. `Integration/`) and `FsyncMacUITests` |
| `apps/macos/scripts` | `build-fsync-mac-app.sh` (signed `FsyncMac.app`), `run-fsync-mac-integration.sh` (Docker Postgres + dev backend + relay + CLI, then the integration tests) |
| `apps/windows` | .NET 8 solution: `Fsync.CoreBindings` (generated C# + native DLL per RID), `Fsync.App.Core` (bridge, DPAPI secrets, view models), `Fsync.App` (WinUI 3 tray app), xUnit tests, UIA e2e script. See its README |
| `scripts` | `gen-bindings.sh` (Swift bindings + headers), `build-xcframework.sh` (universal static lib, xcframework), `gen-bindings-cs.ps1` (C# bindings), `build-windows-dll.ps1` (`fsync_ffi.dll` per RID) |
| `tests/e2e` | `p0-exit-demo.sh`: end-to-end demo (backend, CLI, relay in Docker Postgres) |
| `spikes/iroh-discovery` | Standalone spike: discovery, relay (`relay [port]`), blob measurements. Not in the workspace |
| `spikes/ffi-bindings` | Standalone UniFFI crate + C# harness: 10k-callback stress, buffer copy timing (phase 03) |
| `spikes/windows-clipboard` | Phase 05 spike: listener, sensitive-format filter, virtual-file `IDataObject`, self-test, WinUI 3 tray; evidence in `results/` |
| `deploy/backend` | Backend `Dockerfile`, `compose.yml`, `.env.example` |
| `deploy/relay` | Empty. Relay deployment is phase 10 |
| `docs` | Project documentation (this folder). ADRs in `docs/adr/` |
| `plans` | Implementation plans and reports (`plans/reports/`) |
| `.github/workflows/ci.yml` | Rust job (fmt, clippy `-D warnings`, test) on Ubuntu, macOS, Windows; app jobs are placeholders (`app-macos` does not yet run the Swift tests or the integration script) |

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
| `apps/macos/Packages/FsyncCoreKit/Tests` | Swift XCTests: 8 in `FsyncCoreKitTests`, 14 in `TrustResetPromptTests` |
| `apps/macos/FsyncMac/Tests` | 94 XCTests: 89 in `FsyncMacCoreTests`, 5 in `FsyncMacUITests`. 3 are integration tests that skip unless `FSYNC_IT_API` is set (89 run, 3 skipped, 0 failed) |
| `tests/e2e` | Two-device demo over direct LAN and forced relay |

Current pass/fail state is tracked in [project-changelog.md](./project-changelog.md#verification-status), not here.

## Running the macOS app

Needs `apps/macos/Packages/FsyncCoreKit/FsyncFFI.xcframework` (`scripts/build-xcframework.sh`) and a backend. With `FSYNC_DEV_MODE=1`, sign-in codes are logged as `dev otp` at debug level only (set `RUST_LOG=fsync_backend=debug`; the default level is `info`).

- Dev run: `cd apps/macos/FsyncMac && FSYNC_API=http://127.0.0.1:8080 [FSYNC_RELAY=...] swift run FsyncMac`. The default API base is `http://127.0.0.1:8080`; plain http is allowed only for localhost.
- App bundle: `apps/macos/scripts/build-fsync-mac-app.sh [--api URL] [--relay URL] [--bundle-id ID] [--out DIR] [--version X.Y.Z] [--sign IDENTITY --profile FILE [--notary-profile NAME]]` writes `apps/macos/build/FsyncMac.app` (agent app, hardened runtime). Ad-hoc signed by default; `--sign` + `--profile` give a Developer ID build with the data-protection Keychain entitlements, and `--notary-profile` notarizes and staples. Signing files stay outside the repo (e.g. `~/.fsync-signing/`). The URLs go into `Info.plist` (`FsyncApiBase`, `FsyncRelayUrl`); the env vars override them. `--bundle-id` sets only `CFBundleIdentifier`: the Keychain service and data folder stay `app.fsync.mac`. Launch at login works only from this bundle.
- Secrets (device secret, history key, session) are in the Keychain, service `app.fsync.mac`; data is in `~/Library/Application Support/app.fsync.mac`. Dev reset: `security delete-generic-password -s app.fsync.mac -a <account>` for `device_secret`, `history_key`, `session`, and delete that folder.
- Integration test: `apps/macos/scripts/run-fsync-mac-integration.sh` (needs Docker, cargo, python3, swift, and the xcframework). It cleans up its container, processes and test Keychain items on exit.
- Unit tests: `cd apps/macos/FsyncMac && swift test`.

## Size Notes

Repomix compaction of the repo (262 files, include-filtered to skip build outputs) was generated for this summary in the session scratchpad, not at the repo root. It is not committed.
