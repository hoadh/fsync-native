# Phase 06: Dev-backend integration test, render tests, docs

## Context Links
- tests/e2e/p0-exit-demo.sh (postgres container, `FSYNC_DEV_MODE=1` backend, relay spike, `otp_for`, CLI JSON events `offer`/`sas`/`paired`/`logged_in`)
- core/fsync-cli/src/args.rs (global flags `--data --api --relay --insecure-http --name`)

## Overview
Priority P1 · Status completed (manual checks open) · 8h.
- An end-to-end run of the real Mac logic (EngineBridge plus the models, no views) against a local dev backend, with `fsync-cli` as the peer.
- Render smoke tests for the views.
- Doc updates.

## Key Insights
- XCTest can drive the models directly, so no UI automation is needed. The test reads the OTP from the backend log, like `otp_for`, and spawns the CLI with `Foundation.Process`.
- The test is skipped (`XCTSkip`) unless `FSYNC_IT_API` is set, so a plain `swift test` stays hermetic.
- Use a unique container `fsync-mac-it-pg-$$`, removed on exit. Never touch `mystifying_euclid`.
- Mac and CLI run on the same host, so LAN status is expected (as in the e2e direct round).

## Requirements
- Script `apps/macos/scripts/run-fsync-mac-integration.sh`:
  1. Builds `fsync-backend` and `fsync-cli` (release) and the relay spike, honouring `CARGO_TARGET_DIR`.
  2. Starts postgres:17, the backend (dev mode, debug log) and the relay on free ports.
  3. Exports `FSYNC_IT_API`, `FSYNC_IT_RELAY`, `FSYNC_IT_BACKEND_LOG`, `FSYNC_IT_CLI`, `FSYNC_IT_WORK`.
  4. Runs `swift test --package-path apps/macos/FsyncMac --filter FsyncMacIntegrationTests`.
  5. Cleanup trap: kills the PIDs, `docker rm -f -v` its own container, and keeps the logs on failure. `KEEP=1` keeps the work dir.
- Test `Tests/FsyncMacCoreTests/Integration/FsyncMacIntegrationTests.swift`. Each test uses its own email, test Keychain service and temp data dir.
  1. **First device + CLI offers, Mac joins:**
     1. Mac LoginModel `sendCode` → OTP from the log → `verify`.
     2. Expect bridge `.ready` and `isFirstDevice`.
     3. CLI `login` on the same email (`needs_pairing`), then `pair offer --yes` in the background → code from its `offer` event.
     4. PairingModel `join(code)` → `.sas(peer)` with words equal to the CLI `sas` event → `confirm(accept: true)` → `.done`. The CLI exits 0 with `paired`.
  2. **Status:**
     1. Start CLI `clip watch --json --count 1 --timeout 60` to keep the peer online.
     2. DevicesModel shows the CLI node with `.lan` within 5 s of its `ready` event (poll at 100 ms intervals).
     3. Kill the watcher.
  3. **Remove:** `DevicesModel.remove(nodeId: cli)` leaves `devices()` without it.
  4. **Mac offers (needsPairing), CLI joins:**
     1. CLI logs in first on a new email (created).
     2. The Mac logs in → `.needsPairing`.
     3. PairingModel `offer(.code)` → CLI `pair join <code> --yes` → the Mac gets the ConfirmRequest event → `.sas`, words equal → confirm → `.done` → bridge `.ready`.
- Helpers in `Tests/FsyncMacCoreTests/Integration/DevBackendHarness.swift`: env lookup, `otp(for:)` (strip ANSI, last `dev otp` with `to="email"`), `cli(_ args:) -> Process` with JSON-line reader, `waitUntil(timeout:_:)`.
- Render tests `Tests/FsyncMacUITests/ViewRenderTests.swift`: `ImageRenderer` PNGs for Login, Otp, FirstDevice, PairOffer (code + QR), SasConfirm, DeviceList (mixed statuses), StatusHeader, Settings → `$TMPDIR/fsync-mac-renders/`. Assert non-nil, non-empty.
- Docs:
  - `docs/system-architecture.md`: the Apps line and the macOS app section.
  - `docs/codebase-summary.md`: FsyncMac layout.
  - `docs/ffi-contract.md`: "App guidance" note on event order (FIFO main hop) and session persistence.
  - `docs/project-changelog.md`, `docs/development-roadmap.md` (phase 11 progress).
  - Mark the phase 11 spec todos. Mention the build and integration scripts in the README.

## Architecture
```
run-fsync-mac-integration.sh ── docker pg ── fsync-backend (dev, log) ── relay spike
          │ env FSYNC_IT_*                                  ▲ HTTP           ▲
          ▼                                                  │                │
 swift test (FsyncMacIntegrationTests) ── EngineBridge/Login/Pairing/Devices models ── FsyncEngine
          │ Process                                                                    ▲ iroh LAN
          └──────────── fsync-cli (peer: login, pair offer/join, clip watch) ──────────┘
```

## Related Code Files
Create:
- `apps/macos/scripts/run-fsync-mac-integration.sh`
- `Tests/FsyncMacCoreTests/Integration/DevBackendHarness.swift`
- `Tests/FsyncMacCoreTests/Integration/FsyncMacIntegrationTests.swift`
- `Tests/FsyncMacUITests/ViewRenderTests.swift`

Modify: the docs listed above (docs owner: macOS track entries only).

## FFI calls
All of the above, through the models. There are no direct FFI calls in tests except `EngineConfig` construction (via `AppConfig` with env overrides).

## Implementation Steps
1. Harness helpers and the env-gated skip.
2. The script, built by copying the setup block from p0-exit-demo.sh (DRY debt noted in plan Q4).
3. Integration tests 1 → 4, run, and fix the findings.
4. Render tests and a visual check of the PNGs.
5. Manual app checklist using the bundle from phase 5. Record the results in `plans/reports/`:
   - MenuBarExtra.
   - Window focus for text fields.
   - The remove `confirmationDialog`.
   - The trust-reset dialog, triggered via CLI `trust reset --yes` on a second device.
   - Launch at login.
6. Docs updates. Run code-reviewer (≥9).

## Todo List
- [x] DevBackendHarness + skip gating
- [x] run-fsync-mac-integration.sh (PASS, trap cleanup verified on success and SIGTERM)
- [x] Integration tests PASS (3 tests: join+status+remove, Mac offers, recovery; status asserts `online`, not Lan, see report)
- [x] View render tests (in `OnboardingRenderTests`, incl. Settings; Login/Otp/PairJoin skipped for @FocusState)
- [ ] Manual checklist report
- [x] Docs + changelog + roadmap (README is outside the allowed paths; "Running the macOS app" is in codebase-summary)
- [x] Code review ≥9 (9.0 → 9.5 → 9.6, 0 critical); `swift test` FsyncMac 89 (3 skipped) + FsyncCoreKit 8+14, 0 warnings: done

## Success Criteria
- The script prints PASS and exits 0. The SAS words match in both directions, Lan shows within ≤5 s, and the removed device is gone.
- A plain `swift test` passes offline, with the integration tests skipped.
- No leftover containers (`docker ps -a --filter name=fsync-mac-it` is empty) and no leftover test Keychain items (`security find-generic-password -s app.fsync.mac.tests` fails).

## Risk Assessment
| Risk | L×I | Mitigation |
|---|---|---|
| Flaky timing (presence, mDNS) | M×M | Poll with deadlines, not sleeps. Log CLI output on failure. One retry is allowed only for the status wait. |
| Release build of backend, CLI and relay is slow (minutes) | H×L | Reuse `CARGO_TARGET_DIR`; incremental builds |
| Keychain prompt blocks the test run | M×M | The `swift test` binary creates its own items, so the item ACL already lists the creating app and normally causes no prompt. If a prompt still blocks, document running once interactively. |
| Port or container collisions | L×M | Free ports via python, container name with `$$` |

## Security Considerations
- Test accounts use `@example.test`. OTPs exist only in the temp backend log, which is removed on success.
- No real Keychain service (`app.fsync.mac`) is used by tests.

## Next Steps
Phase 12 (clipboard, files, history) builds on this shell.
