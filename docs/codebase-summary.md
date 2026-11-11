# Codebase Summary

Last updated: 2026-11-09. Repo map for the P0 Rust track, the production deploy stack, the macOS app (clipboard, files, history, settings), the Windows app and the Linux desktop app (`apps/linux`, ADR 014). Architecture is in [system-architecture.md](./system-architecture.md); standards in [code-standards.md](./code-standards.md).

## Repo Map

| Path | Purpose |
|------|---------|
| `core/fsync-proto` | Wire types and validation: device list, pairing frames, clip/file frames, API DTOs, presence, names, caps |
| `core/fsync-core` | Client engine: identity, trust, transport (iroh), pairing, clip and file sync, encrypted history, `FsyncService` facade |
| `core/fsync-ffi` | UniFFI 0.29.4 bindings over the facade (`FsyncEngine`, contract v3) |
| `core/fsync-cli` | `fsync-cli` binary: login, pair, devices, clip, send, fetch, history, bench |
| `backend` | `fsync-backend` (axum 0.8, sqlx 0.8 Postgres): OTP auth, device-list registry, pairing codes, presence WebSocket |
| `backend/migrations` | `0001_init`, `0002_refresh_session` |
| `backend/tests` | Integration tests against Postgres via testcontainers |
| `apps/macos/Packages/FsyncCoreKit` | SwiftPM package: `FsyncCoreKit` Swift wrapper and XCTests over the xcframework; `TrustResetPrompt` SwiftUI library (trust-reset prompt) with its tests |
| `apps/macos/FsyncMac` | SwiftPM app (tools 6.0, macOS 13): `FsyncMacCore` (`Bridge/` EngineBridge and Keychain; `Clipboard/` watcher, sensitive filter, mapper, writer; `Files/` file clips, fetch, transfers, inbox; `Settings/`; `Features/` onboarding, pairing, devices, history), `FsyncMacUI` (SwiftUI: `Menu/`, `History/`, `Devices/`, `Settings/`, `Pairing/`, `Onboarding/`), `FsyncMac` (`@main` menubar app); tests in `FsyncMacCoreTests` (incl. `Integration/`) and `FsyncMacUITests` |
| `apps/macos/scripts` | `build-fsync-mac-app.sh` (signed `FsyncMac.app`), `run-fsync-mac-integration.sh` (Docker Postgres + dev backend + relay + CLI, then the integration tests) |
| `apps/windows` | .NET 8 solution: `Fsync.CoreBindings` (generated C# + native DLL per RID), `Fsync.App.Core` (bridge, DPAPI secrets, view models), `Fsync.App` (WinUI 3 tray app), xUnit tests, UIA e2e script. See its README |
| `apps/linux/app` | `fsync-linux` binary (Qt6/QML over CXX-Qt, own Cargo workspace): `src/bridge/` (all CXX-Qt bridges, one directory), `src/engine/` (boot, ops, clipboard runtime, inbox, offers, send, shutdown, tray, IPC handler, e2e control), `src/core_state/`, `qml/` (views and models), `cpp/` (smoke logger), `build.rs`. Feature `e2e-control` (test only) and `emit-platform-linux` (off) |
| `apps/linux/platform` | `fsync-linux-platform` crate, no Qt: `clipboard/` (Wayland `ext`/`zwlr` data-control, X11 `x11rb`, detection, sensitive filters, file-clip path guard, echo guard), `secrets/` (oo7 Secret Service, 0600 file fallback, backend marker), `ipc/` (single-instance socket, peer check), `sync/` (lanes, inbound, outbound, file inbox sharing), `files/`, `instance_lock.rs`, `autostart.rs`, `notify.rs`, `paths.rs`, `config_file.rs`. Tests in `tests/` (Wayland, X11 and sway/Xvfb backends, skipped when the tools are missing) |
| `apps/linux/packaging` | `arch/PKGBUILD` (builds from source against system Qt and OpenSSL), `appimage/build-appimage.sh` (linuxdeploy plus Qt plugin, Wayland plugins copied by hand), `about.hbs` (third-party license report template) |
| `apps/linux/scripts` | `build-release.sh` (one binary in the `ubuntu:22.04` container, then `.deb`, `.rpm`, AppImage, `SHA256SUMS`), `build-in-container.sh`, `run-in-container.sh`, `package-in-container.sh`, `release-guard.sh`, `smoke-install.sh` (clean-container install checks), `check-lock-drift.sh` (fsync-core deps equal root lock), `check-assets.sh` (deb/rpm assets equal `lib/assets.list`), `derive-runtime-deps.sh`, `lib/` (helpers, `assets.list`), `container/` (Dockerfiles, pinned tools) |
| `apps/linux/tests/e2e` | `clipboard-with-cli.sh` (two app instances and an fsync-cli device in nested headless sway and Xvfb), `pair-with-cli.sh` (sign-in and pairing), `cross-os-with-windows.sh` (phase 06: X1 local, X2 and X5 with fsync-cli on waso-vm, X4 and X3 with the Windows GUI app via `X3=1`, through split `remote run`s; sourced helpers `lib-cross-os-windows-legs.sh`, `lib-cross-os-gui-legs.sh`), `lib-*.sh` shared helpers. Results go to `docs/test-results/linux-cross-os-*.json` |
| `apps/windows/tests/e2e/cross-os-cli-peer.ps1` | Phase 06 Windows peer (headless `fsync-cli`, Windows PowerShell 5.1): `-Step build \| pair \| leg`, JSON lines on stdout. Driven by the Linux harness |
| `apps/windows/tests/e2e/cross-os-gui-peer.ps1` | Phase 06 X3/X4 Windows peer (the WinUI app driven by UI Automation, Windows PowerShell 5.1): `-Step build-app \| pair \| legs`; re-runs itself as a one-shot scheduled task in the logged-on desktop session. Driven by the Linux harness |
| `apps/windows/tests/e2e/cross-os-port-forward.ps1` | Verify-only check of the reverse SSH tunnel that gives waso-vm the host's `127.0.0.1` API and relay ports. Changes nothing |
| `scripts` | `gen-bindings.sh` (Swift bindings + headers), `build-xcframework.sh` (universal static lib, xcframework), `gen-bindings-cs.ps1` (C# bindings), `build-windows-dll.ps1` (`fsync_ffi.dll` per RID) |
| `tests/e2e` | `netns/` and `netns-matrix.sh`: N1-N9 in network namespaces (privileged Docker; the relay runs TLS with a private CA and QAD, see [test results](./test-results/README.md)), results in `docs/test-results/`. `p0-exit-demo.sh`: end-to-end demo (backend, CLI, relay in Docker Postgres). `compose-stack-sync.sh`: the production compose stack under Docker (Caddy with a local CA, two CLIs, relay-forced clip and file, backup and restore) |
| `spikes/iroh-discovery` | Standalone spike: discovery, relay (`relay [port]`), blob measurements. Not in the workspace |
| `spikes/ffi-bindings` | Standalone UniFFI crate + C# harness: 10k-callback stress, buffer copy timing (phase 03) |
| `spikes/macos-clipboard` | Phase 04 spike (own SwiftPM package, not in the workspace): pasteboard watcher, sensitive filter, lazy file writer, `ClipSpikeSelfTest` child-process self-test; results in `results/`. ADR 009 |
| `spikes/windows-clipboard` | Phase 05 spike: listener, sensitive-format filter, virtual-file `IDataObject`, self-test, WinUI 3 tray; evidence in `results/` |
| `deploy` | Production stack: `compose.yml` (Postgres 17, backend, relay; loopback ports only), `compose.local.yml` (local testing overlay: Caddy with a local CA), `Caddyfile`, `.env.example`, `README.md` (runbook) |
| `deploy/backend`, `deploy/relay` | Images: backend `Dockerfile` (cross-compiled, build context = repo root); relay image from `iroh-relay` 1.3.0 with `entrypoint.sh`. `deploy/backend/compose.yml` is the older dev-only file |
| `deploy/scripts` | `deploy.sh` (flock, staging, rollback), `smoke-check.sh`, `backup-postgres.sh`, `restore-postgres.sh`, `gen-env.sh`, `bootstrap-vps.sh` (dedicated VPS only). Architecture: [system-architecture](./system-architecture.md#deployment-topology) |
| `docs` | Project documentation (this folder). ADRs in `docs/adr/` |
| `plans` | Implementation plans and reports (`plans/reports/`) |
| `.github/workflows/ci.yml` | `changes` (path filter), `rust` (fmt, clippy `-D warnings`, test; always runs), `macos` (`swift test`) and `windows` (DLL and C# bindings, `dotnet test`) run only when their inputs change, and always on `workflow_dispatch` and push to `main`. The integration script needs Docker, which macOS runners lack, so CI runs only the unit tests |
| `.github/workflows/deploy-stack.yml` | Builds `fsync-backend` and `fsync-relay`, lints the deploy scripts (shellcheck) and compose file, runs `tests/e2e/compose-stack-sync.sh` when deploy inputs change |

Generated and local (not source): `target/` (Cargo), `build/` (script output: `ffi-headers`, `universal`), `*.xcframework`, `.env`.

## Crate Layout

### core/fsync-proto
`api_dto`, `clip`, `device_list`, `error`, `file`, `fixed_bytes`, `frame` (max frame 1 MiB + 256 KiB), `names`, `node_id`, `pairing`, `presence`, `shape`, `sig`, `verify`.

### core/fsync-core
| Module | Purpose |
|--------|---------|
| `api` | HTTP client (HTTPS only unless dev flag): `check` (HTTP outcome to `CoreError`; `reachability` tells an unreachable backend from a TLS or HTTP failure), `client/authed` (refresh-on-401), presence session, WS utilities |
| `blobs` | Blob store trait, plain FS store, `EncryptedBlobStore` (FSB1), export |
| `clip` | Clip offers, echo guard, rate limit |
| `config` | `EngineConfig` |
| `engine` | Engine runtime and events |
| `file` | File transfer (resume, ranges, locks, policy). Read-ahead: `range_reader` (4 MiB windows, prefetch), `seq_run` (sequential-read detection), `reader_cache` (LRU of readers, per-peer prefetch budget of 8) |
| `history` | SQLCipher history DB, keycheck, backups, retention |
| `identity` | Node identity keys |
| `net` | iroh endpoint, relay URL and path handling. `lan_first_path_selector` (custom iroh `PathSelector`: LAN, then other direct, then relay), `local_subnets` (on-link LAN subnets from physical, up, gatewayed interfaces), `subnet_cache` (5 s background refresh, never blocks `select()`), `path_tier` (best live tier per peer: lan, direct, relay) |
| `pair` | Pairing windows, SPAKE2/HKDF initiator and responder steps |
| `service` | `FsyncService` facade (login, pairing, devices, clips, transfers, history, presence). `lifecycle` (start/stop, `engine.lock`), `revoke` (logout revocation with background retry), `list_freshness` (`list-synced-at` stamp and the 7-day offline-start policy), `path_tier` (`peer_path_tier` facade) |
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
| `core/fsync-core/tests` | Pairing, trust, clip and file protocols, history, settings, service facade, security limits, read-ahead (`file_read_ahead.rs`), data-dir lock (`data_dir_lock.rs`), logout revocation (`logout_revocation.rs`, behind a TCP forwarder) |
| `core/fsync-ffi/tests` | Error mapping round-trip |
| `core/fsync-cli/tests` | CLI surface (help, required flags, exit codes) |
| `backend/tests` | Auth, refresh, devices, reset, presence, pairing, limits (Postgres testcontainers) |
| `apps/linux/platform/tests` | Clipboard backends (Wayland, X11, sway and Xvfb headless), IPC, secrets, sync lanes, path guard. Run with `cargo test -p fsync-linux-platform` in `apps/linux` |
| `apps/macos/Packages/FsyncCoreKit/Tests` | Swift XCTests: 8 in `FsyncCoreKitTests`, 14 in `TrustResetPromptTests` |
| `apps/macos/FsyncMac/Tests` | XCTests for `FsyncMacCoreTests` (clipboard, file clips and transfers, history, settings, bridge) and `FsyncMacUITests`. `Integration/` skips unless `FSYNC_IT_API` is set |
| `tests/e2e` | Two-device demo over direct LAN and forced relay; compose stack run (`compose-stack-sync.sh`) |

Current pass/fail state is tracked in [project-changelog.md](./project-changelog.md#verification-status), not here.

## Running the macOS app

Needs `apps/macos/Packages/FsyncCoreKit/FsyncFFI.xcframework` (`scripts/build-xcframework.sh`) and a backend. With `FSYNC_DEV_MODE=1`, sign-in codes are logged as `dev otp` at debug level only (set `RUST_LOG=fsync_backend=debug`; the default level is `info`).

- Dev run: `cd apps/macos/FsyncMac && FSYNC_API=http://127.0.0.1:8080 [FSYNC_RELAY=...] swift run FsyncMac`. The default API base is `http://127.0.0.1:8080`; plain http is allowed only for localhost.
- App bundle: `apps/macos/scripts/build-fsync-mac-app.sh [--api URL] [--relay URL] [--bundle-id ID] [--out DIR] [--version X.Y.Z] [--sign IDENTITY --profile FILE [--notary-profile NAME]]` writes `apps/macos/build/FsyncMac.app` (agent app, hardened runtime). Ad-hoc signed by default; `--sign` + `--profile` give a Developer ID build with the data-protection Keychain entitlements, and `--notary-profile` notarizes and staples. Signing files stay outside the repo (e.g. `~/.fsync-signing/`). The URLs go into `Info.plist` (`FsyncApiBase`, `FsyncRelayUrl`); the env vars override them. `--bundle-id` sets only `CFBundleIdentifier`: the Keychain service and data folder stay `app.fsync.mac`. Launch at login works only from this bundle.
- Secrets (device secret, history key, session) are in the Keychain, service `app.fsync.mac`; data is in `~/Library/Application Support/app.fsync.mac`. Dev reset: `security delete-generic-password -s app.fsync.mac -a <account>` for `device_secret`, `history_key`, `session`, and delete that folder.
- Integration test: `apps/macos/scripts/run-fsync-mac-integration.sh` (needs Docker, cargo, python3, swift, and the xcframework). It cleans up its container, processes and test Keychain items on exit. CI cannot run it (no Docker on macOS runners).
- Unit tests: `cd apps/macos/FsyncMac && swift test`.

## Size Notes

Repomix compaction of the repo (262 files, include-filtered to skip build outputs) was generated for this summary in the session scratchpad, not at the repo root. It is not committed.
