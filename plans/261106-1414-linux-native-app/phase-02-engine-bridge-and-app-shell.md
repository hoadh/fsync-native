# Phase 02 — Tolerant Platform decode + engine bridge + app shell (single instance, tray, sign-in, pairing, devices, secrets, autostart)

## Context Links
- [plan.md](plan.md), [phase-01](phase-01-spike-and-scaffold.md), [clipboard/secrets research](../reports/researcher-261106-1354-linux-clipboard-hotkeys-secrets.md) §6, §7, [red team](reports/red-team-261106-1438-plan-review.md) #1 #2 #3 #6 #11 #12 #15
- Facade: `core/fsync-core/src/service/mod.rs` (blocking methods, any non-runtime thread; service is `Sync`), `delegate.rs` (`ServiceDelegate`, one "fsync-callbacks" thread), `config.rs` (`ServiceConfig`), `lifecycle.rs` (`start()` takes `engine.lock`), `login.rs`, `pairing.rs`, `devices.rs`, `trust_reset.rs`, `revoke_session.rs`
- Parity: `apps/windows/README.md` (secrets.dat contents, `--background`, env names), `apps/windows/tests/e2e/pair-with-cli.ps1`, ADR 013 decision 14 (one engine per data dir, `EngineAlreadyRunning`)
- E2E helpers: `tests/e2e/lib/cli-session.sh`, `tests/e2e/lib/assert.sh`, `tests/e2e/p0-exit-demo.sh` (backend + relay start)
- Platform wire format: `core/fsync-proto/src/device_list.rs` (`Platform`, `SignedDeviceList::{decode,hash}`, `DeviceList::successor`), `core/fsync-proto/src/pairing.rs` (`PairFrame::DeviceInfo { platform }`), `core/fsync-proto/src/verify.rs`, `core/fsync-core/src/trust/ingest.rs`, `backend/src/devices/verify.rs`, `core/fsync-ffi/src/types_device.rs`

## Overview
- Priority: P1. Status: In Progress (code done, review 9.5/10; LIVE checks open). Depends on 01 go. Estimate: 7 d (1 d core Platform commit).
- Working app without clipboard: sign in (email OTP), pair (code/QR/SAS), device list with status, remove device, trust-reset prompt, sign out (pending revoke), tray, autostart, single instance with IPC forwarding.

## Key Insights
- <!-- red-team 2026-11-06 #1 --> `Platform` is a plain serde enum (postcard varint index 0..=2). Today an unknown index fails `SignedDeviceList::decode` -> `verify_chain`/`ingest.rs` reject every list containing it (stuck chain head, revocations never ingested) and `PairFrame::DeviceInfo` fails to decode; backend `verify.rs` (same crate) returns 422. So `Platform::Linux` must never be emitted until every peer tolerates it.
- Two-step rollout (user decision 2026-11-06): this plan ships a **forward-compatible decoder** and a `Linux` definition nobody emits; the Linux app sends `Platform::Cli`. Emitting `Linux` is a later release behind the plan Gate **Platform emit gate** (prod backend + Mac/Windows releases with tolerant decoder deployed). Backend picks up the decoder in its next regular deploy; no special prod gate since `Cli` is already valid.
- Verified 2026-11-06 in code: signatures cover `SignedDeviceList.body` bytes (`check_sig`), `hash()` = blake3 of postcard(`SignedDeviceList`) = original `body` bytes + `sig`; decoding never re-encodes for hashing or verification; replay check compares bytes. The only re-encode is `DeviceList::successor()` (decode prev -> build next list -> new body). Therefore the unknown variant must **round-trip its original index** (`Unknown(u32)` serializes back to the same varint), otherwise an old-but-tolerant client removing/adding a device would silently rewrite a Linux entry as `Cli`. `DEVLIST_VERSION` stays 1: known variants keep identical bytes; a bump would itself break old clients.
- `FsyncService` methods block. <!-- red-team 2026-11-06 #12 --> No single serial worker: long ops (`login_*`, `pairing_offer`/`pairing_join` up to 60 s, `devices`, `logout`) each run on their own short-lived thread with a per-op generation id; control calls (`pairing_cancel`, `cancel`, `stop`) are called directly from a fresh thread (service is `Sync`), never queued behind a long op. Clipboard publish gets its own lane in 03.
- <!-- red-team 2026-11-06 #2 --> Secrets backend must be decided once per data dir: re-detecting each launch (slow/locked keyring at autostart) would create new keys -> new node id + `HistoryKeyMismatch`. Persist choice in `$data_dir/secrets-backend`; never create keys when `history.db` or the marker exists. Secret Service calls are blocking D-Bus: never on core's callback thread (bounded queue 4096 drops), always on a dedicated `fsync-secrets` thread with timeouts.
- <!-- red-team 2026-11-06 #3 --> `engine.lock` is taken in `start()`, not `new()`, so it cannot gate the IPC socket. App takes its own `$data_dir/app.lock` (flock) first thing in `main`; only the holder binds/unlinks the socket.
- GNOME without AppIndicator has no tray: main window must always be reachable (launcher re-activation via IPC `show`).

## Requirements
Functional:
- Core: `Platform::{Mac, Win, Cli, Linux, Unknown(u32)}` with custom `Serialize`/`Deserialize` (varint index; 3 = `Linux`; >= 4 -> `Unknown(n)`, re-serialized as `n`); `Platform::wire_compat()` maps `Linux`/`Unknown` -> `Cli` for display/FFI. Applies to `DeviceEntry` and `PairFrame::DeviceInfo` (same type). <!-- red-team 2026-11-06 #1 -->
- Linux app emits `Cli` unless built with cargo feature `emit-platform-linux` (default off, not enabled by any packaging script). <!-- red-team 2026-11-06 #1 -->
- CLI: `fsync-linux [--background] [--data-dir D] [--show] [--clip-send] [--clip-paste] [--send <paths>...] [--version] [--build-info] [--smoke]`. Second launch forwards to running instance and exits 0. `show`/`send` queued until signed in; `clip-*` rejected with `{"ok":false,"error":"signed-out"}` when not Ready (no queue). <!-- red-team 2026-11-06 #3 -->
- Config: env (same as Windows/CLI) `FSYNC_API`, `FSYNC_RELAY`, `FSYNC_DEVICE_NAME` (default hostname), `FSYNC_DATA_DIR` (default `$XDG_DATA_HOME/fsync`); packaged builds read `$XDG_CONFIG_HOME/fsync/config.toml` (`api`, `relays`), env overrides file; built-in default = production API (not loopback). <!-- red-team 2026-11-06 #15 -->
- Views: SignInView (email -> code), PairingView (show code / show QR / join by code), SasDialog (3 words, confirm/cancel), DevicesView (name, platform, status Lan/Relay/Offline, remove), trust-reset prompt, sign-out. Errors via `user_messages.rs` (FsyncError -> text; never raw Debug).
- Tray (ksni): icon, menu Open / Pause sync (wired in 03) / Quit; left click toggles window. Retry registration when StatusNotifierWatcher appears (D-Bus NameOwnerChanged).
- Secrets: device secret, history key, session tokens, email, pending revoke. First run: Secret Service (oo7) if `org.freedesktop.secrets` answers within 2 s, else 0600 file `$data_dir/secrets.json` + persistent warning banner; choice written to `$data_dir/secrets-backend` (`secret-service`|`file`). Later runs use the marker only; Secret Service unavailable/locked -> state `WaitingForKeyring` (banner, retry every 10 s + on `NameOwnerChanged`), never fall back, never create keys. <!-- red-team 2026-11-06 #2 -->
- Autostart toggle: writes/removes `$XDG_CONFIG_HOME/autostart/app.fsync.Fsync.desktop` with `Exec=fsync-linux --background`.
- Notifications via notify-rust (pairing done, errors).
- Shutdown with wall-clock deadline (see Architecture). <!-- red-team 2026-11-06 #11 #12 -->
Non-functional: UI never blocks > 16 ms on engine; all files < 200 lines; unit tests for ipc, secrets (marker + fallback), paths, user_messages, autostart, Platform codec.

## Architecture
```
QML view --invokable--> EngineObject (GUI thread) --spawn op thread (gen id)--> FsyncService (blocking)
                              ^                                                        |
                              +---- CxxQtThread.queue(set props / emit; stale gen dropped) <+
QML Cancel/Quit --------> control thread --> service.pairing_cancel()/stop() (direct, Sync)
core "fsync-callbacks" thread --> DelegateAdapter: on_session_changed -> secrets_tx.send + wait <= 3 s
                                                   others -> queue -> EngineObject signals / DevicesModel
fsync-secrets thread  <-- jobs (load/save/clear, 5 s timeout each) --> oo7 | file_fallback
second process --> ipc::client --Unix socket--> ipc::server thread --> queue -> EngineObject.handleCommand
```
- `EngineObject` (QObject, singleton in QML): props `state` (Starting/WaitingForKeyring/SignedOut/AwaitingCode/Ready/Error), `email`, `nodeId`, `banner`; invokables `loginStart(email)`, `loginVerify(code)`, `logout()`, `confirmTrustReset(id)`, `declineTrustReset(id)`; signal `showWindowRequested(activationToken)`.
- `PairingObject`: `offerCode()`, `offerQr()` (QR PNG via `qrcode` crate -> `data:image/png;base64` URL; no qt6-svg dep), `join(input)`, `confirm(requestId, yes)`, `cancel()` (direct control call); props `code`, `qrUrl`, `sas`, `peerName`, `phase`.
- `DevicesModel`: `QAbstractListModel` (roles name, platform, status, nodeId); refreshed from `devices()` and patched by `on_device_status`.
- Start sequence: `app.lock` flock (held -> IPC client path) -> paths -> secrets via marker (or first-run detect + create keys only if no `history.db` and no marker) -> `FsyncService::new` -> `set_session` if tokens -> `start` (on `EngineAlreadyRunning` -> forward via IPC, exit) -> `ensure_account` -> devices -> retry pending revoke. <!-- red-team 2026-11-06 #2 #3 -->
- Token persist: callback thread sends `Save` to `fsync-secrets` and waits up to 3 s for the ack (keeps order); timeout -> keep tokens in memory, banner "secrets not saved", retry on next change/quit. Never call oo7 on the callback thread. <!-- red-team 2026-11-06 #2 -->
- IPC (`platform/src/ipc`): socket `$XDG_RUNTIME_DIR/fsync/<blake3(data_dir canonical)[..16]>.sock`. <!-- red-team 2026-11-06 #3 --> Before bind/connect verify the dir: `lstat` not a symlink, owner == our uid, mode 0700 (create if missing); fails -> IPC disabled (logged, banner), app still runs. No `/tmp` fallback when `XDG_RUNTIME_DIR` unset: use `$data_dir/run/` with the same checks. Only the `app.lock` holder unlinks a stale socket and binds; a client that sees the lock held but cannot connect retries 20 x 100 ms. Socket 0600, `SO_PEERCRED` uid == ours else drop. Protocol: one JSON line `{"cmd":"show"|"clip-send"|"clip-paste"|"send","paths":[...],"activation_token":"..."?}`, reply `{"ok":true}|{"ok":false,"error":"..."}`; max line 64 KiB, max 1000 paths, abs + existing paths only (re-validated server side). Client forwards `XDG_ACTIVATION_TOKEN`/`DESKTOP_STARTUP_ID` when set (used by 03 GNOME mini window). <!-- red-team 2026-11-06 #7 -->
- Test commands (feature `e2e-control`, minimal set): `status`, `login`, `pair-offer`, `pair-join`, `pair-confirm`; 04a/04b may add `offer-accept`, `pick-device`. <!-- red-team 2026-11-06 #6 --> `app/build.rs` emits `cfg(fsync_allow_test_features)` when env `FSYNC_ALLOW_TEST_FEATURES=1`; `app/src/main.rs` has `#[cfg(all(feature = "e2e-control", not(debug_assertions), not(fsync_allow_test_features)))] compile_error!(...)`. `--build-info` prints version, git sha, enabled features (`e2e-control`, `emit-platform-linux`) as JSON.
- Shutdown: Quit -> close lifecycle gate (no new ops/IPC; 03 hooks clipboard here) -> stop IPC server -> `stop()` on a control thread (3 s) -> secrets flush (2 s) -> drop service -> `QGuiApplication::quit`; global wall-clock deadline 6 s, then `std::process::exit(0)` after logging which step hung. <!-- red-team 2026-11-06 #11 #12 -->

## Related Code Files
Create (all under `/home/alex/Work/fsync-native/apps/linux/`):
- `platform/src/paths.rs`, `platform/src/config_file.rs`, `platform/src/instance_lock.rs`, `platform/src/secrets/{mod.rs,secret_service.rs,file_fallback.rs,backend_marker.rs,secrets_thread.rs}`, `platform/src/ipc/{mod.rs,protocol.rs,server.rs,client.rs,runtime_dir_check.rs}`, `platform/src/autostart.rs`, `platform/src/notify.rs`
- `app/src/cli_args.rs`, `app/src/tray.rs`, `app/src/shutdown.rs`, `app/src/bridge/{engine_object.rs,engine_ops.rs,delegate_adapter.rs,pairing_object.rs,devices_model.rs,user_messages.rs,qr_image.rs}`
- `app/qml/{SignInView.qml,PairingView.qml,SasDialog.qml,DevicesView.qml,TrustResetDialog.qml,StatusBanner.qml}`
- `resources/app.fsync.Fsync.desktop`, `resources/icons/hicolor/{scalable,256x256,48x48}/apps/app.fsync.Fsync.*`, `resources/icons/tray/*.png`
- `tests/e2e/pair-with-cli.sh`
Modify: `apps/linux/app/src/main.rs` (lock, compile_error guard, `--build-info`), `app/src/bridge/mod.rs`, `app/qml/Main.qml`, `app/build.rs` (`fsync_allow_test_features` cfg), both crate `Cargo.toml`s (features `e2e-control`, `emit-platform-linux`; deps oo7, ksni, notify-rust, qrcode, image(png only), serde_json, toml, blake3, nix/rustix for peercred + flock)
Delete: `apps/linux/app/src/bridge/hello_object.rs`
Core (separate commit, `feat(proto): forward-compatible Platform decoding`): <!-- red-team 2026-11-06 #1 -->
- `/home/alex/Work/fsync-native/core/fsync-proto/src/device_list.rs` (custom serde for `Platform`, `Linux` + `Unknown(u32)`, `wire_compat()`; if > 200 lines move codec to new `core/fsync-proto/src/platform.rs` and re-export)
- `/home/alex/Work/fsync-native/core/fsync-ffi/src/types_device.rs` (map `Linux | Unknown(_) -> Cli` via `wire_compat()`; doc comment why). `From<ffi::Platform>` unchanged (never produces Linux)
- `/home/alex/Work/fsync-native/docs/ffi-contract.md` (note: Linux/unknown devices appear as `Cli` through FFI)
- exhaustive `match` on `Platform` found by `cargo build --workspace` (backend, cli, `trust_reset.rs` tests)

## Implementation Steps
1. Core commit <!-- red-team 2026-11-06 #1 -->: custom `Serialize`/`Deserialize` for `Platform` (serde `serialize_unit_variant` with index; deserialize via `deserialize_enum` visitor reading `variant_seed` as u32 index, unit content). Tests: (a) known variants encode to the same bytes as the old derive (golden bytes from a list built before the change); (b) list containing index 3 and index 9 decodes, `Unknown(9)` re-encodes to identical body bytes; (c) `successor()` over such a list keeps entries' indices; (d) `verify_chain`/`ingest` accept a chain with an unknown-platform entry; (e) `PairFrame::DeviceInfo` with index 3 decodes; (f) backend `verify.rs` accepts it. `DEVLIST_VERSION` unchanged. `cargo fmt/clippy/test --workspace` green; p0 demo `MIB=8` PASS. No code path in any app emits `Linux`.
2. `paths.rs`, `config_file.rs` (toml, env override), `instance_lock.rs` (flock, held -> `Err(Held)`).
3. Secrets: `SecretStore` trait (`load`, `save`, `clear`); `secret_service.rs` (oo7, attributes `app=app.fsync`, `data_dir=<hash>`); `file_fallback.rs` (atomic tmp+rename, 0600, refuse if mode wider or owner != uid); `backend_marker.rs` (read/write marker; "keys may be created" only if no marker and no `history.db`); `secrets_thread.rs` (job channel, per-call 5 s timeout, `WaitingForKeyring` retry). `FSYNC_SECRETS=file` honored only in debug or `e2e-control` builds (test-only). Values zeroized. Unit tests: marker present + keyring unavailable -> waiting, no key creation; first run without Secret Service -> file + marker.
4. IPC server/client + `runtime_dir_check.rs` + unit tests (stale socket only unlinked by lock holder, symlinked dir refused, wrong owner/mode refused, oversize line, relative path rejected, `clip-*` signed out rejected, client retry while holder binds).
5. `engine_ops.rs` (spawn per op, generation ids, control calls direct), `delegate_adapter.rs` (implements `ServiceDelegate`, `CxxQtThread` clones + secrets channel).
6. `engine_object.rs`, `pairing_object.rs`, `devices_model.rs`, `user_messages.rs` (exhaustive over `FsyncError`, includes `PeerRefused`, `EngineAlreadyRunning`, `ReauthRequired`).
7. QML views (Basic style, Qt 6.2 APIs only), `Main.qml` StackView by `engine.state`; window close hides only if tray registered, else minimizes; Quit always in menu and tray.
8. `tray.rs` (ksni on its own thread; menu actions -> Qt queue). Watcher-appear retry.
9. `autostart.rs` + Settings stub toggle. `notify.rs` thin wrapper.
10. `main.rs`: compile_error guard, arg parse, `--build-info`, `app.lock` -> holder starts Qt + engine, else IPC forward. `--background` = no window. `shutdown.rs` (gate, ordered steps, 6 s deadline).
11. `tests/e2e/pair-with-cli.sh` (build `--features e2e-control`, debug): <!-- red-team 2026-11-06 #15 --> prereq check (`dbus-run-session`, postgres, built `fsync-cli`; missing -> exit 2 with hint); app runs inside `dbus-run-session` with `FSYNC_SECRETS=file`, offscreen QPA, own data dir + `XDG_RUNTIME_DIR` tempdir. Start Postgres + backend + relay as p0 does; CLI devA login; app login via IPC `login`, `pair-offer`; CLI `pair join <code> --yes`; app `pair-confirm`; assert both `devices` lists include each other and app shows as `Cli` on the CLI; second launch `--show` exits 0 and forwards; `pair-offer` then IPC `cancel` returns within 1 s.
12. Manual on Omarchy: tray in Waybar, sign in against local backend, pair with fsync-cli by QR payload text and by code, SAS words match; quit during an open pairing exits < 6 s.

## Todo List
- [x] Core: tolerant `Platform` codec (`Linux`, `Unknown(u32)`), FFI map, golden + round-trip tests
- [x] paths, config.toml, instance lock
- [x] secrets: marker, secrets thread, oo7 + fallback, waiting state
- [x] IPC server/client + dir checks + peercred + tests
- [x] engine_ops (per-op threads, direct control) + delegate_adapter
- [x] EngineObject, PairingObject (QR PNG), DevicesModel, user_messages
- [x] QML: SignIn, Pairing, SAS, Devices, TrustReset, Banner
- [x] Tray (ksni) with watcher retry
- [x] Single instance + forwarding + `--background` + shutdown deadline
- [x] `e2e-control` compile guard + `--build-info`
- [x] `tests/e2e/pair-with-cli.sh` PASS
- [x] Manual Omarchy run: tray clicks, real login, QR, Wayland raise pass 2026-11-12 (ADR 014 "LIVE run"; Pause sync check mark does not update: finding F2)

## Success Criteria
- `cargo test --manifest-path apps/linux/Cargo.toml -p fsync-linux-platform` green; clippy `-D warnings` clean on apps/linux workspace.
- `cargo test --workspace` (root) green incl. Platform golden-bytes + unknown-variant round-trip tests; `tests/e2e/p0-exit-demo.sh` with `MIB=8` PASS.
- `grep -rn "Platform::Linux" core apps backend --include=*.rs` shows only the enum/codec, tests, and code under `cfg(feature = "emit-platform-linux")`.
- `cargo build --release --features e2e-control --manifest-path apps/linux/Cargo.toml` fails with the guard message; same with `FSYNC_ALLOW_TEST_FEATURES=1` succeeds.
- `apps/linux/tests/e2e/pair-with-cli.sh` -> `PASS` (twice in a row).
- `build-in-container.sh ubuntu-2204` smoke still exit 0 (all new QML loads on 6.2).
- Second `fsync-linux --show` exits 0 in < 1 s and raises the window.

## Pending Live Verification
<!-- red-team 2026-11-06 #14 --> KDE tray / KWallet -> ADR 014 "untested".
- [x] LIVE (pass 2026-11-12, G1/G2): GNOME VM without AppIndicator: no tray, window reachable via launcher (IPC show); Secret Service via gnome-keyring, relaunch after keyring unlock keeps node id — evidence: ADR 014 table

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Custom `Platform` serde changes bytes of known variants | L x H | Golden-bytes test vs pre-change encoding; hashes/sigs stay over original body bytes |
| Old tolerant client rewrites unknown platform on successor | M x M | `Unknown(u32)` re-encodes its index; test (c) |
| Someone flips emit before peers deploy decoder | L x H | Feature default off, not in packaging; Platform emit gate in plan + ADR 014 |
| Secret Service slow at autostart -> new keys | M x H | Marker + never create when `history.db` exists; waiting state |
| Token persist blocked by D-Bus | M x H | Dedicated secrets thread, bounded wait, in-memory retry |
| Socket race / squat | M x M | `app.lock` holder-only bind/unlink, dir owner/mode/symlink checks, no `/tmp` |
| Long op blocks Cancel/Quit | M x H | Per-op threads, direct control calls, 6 s quit deadline |
| CXX-Qt list model API gaps | M x M | Fall back to QVariantList property for devices (small lists) |
| oo7 pre-1.0 API churn | M x L | Exact pin; wrapper trait isolates it |
| Tray invisible (Waybar starts late, GNOME) | H x L | Watcher retry; window always reachable |

## Security Considerations
- Socket: verified 0700 dir (no symlink, our uid), 0600 socket, `SO_PEERCRED`, size caps, path validation; `clip-*` refused while signed out.
- `e2e-control` (incl. `pair-confirm`, a SAS bypass) cannot compile into a release build; `--build-info` lets packaging verify.
- Secrets never logged; zeroize; fallback file permission checks; banner makes the weaker mode visible; marker prevents silent key regeneration.
- Plain http backend only for loopback (core `allow_insecure_http` only when API host is loopback).

## Rollback
- App: revert app commits (nothing outside apps/linux).
- Core Platform commit: safe to revert as long as no build has emitted index >= 3 (true by construction: emit feature off). Reverting after any peer deployed it is also safe because wire bytes of known variants are unchanged. Backend: redeploy previous image if needed; no data migration.

## Next Steps
- Phase 03 adds `ClipboardObject`, clipboard lane and lifecycle hooks on the shutdown gate. Emitting `Linux`: later release (plan Gate **Platform emit gate**).
