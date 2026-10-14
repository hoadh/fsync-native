# Docs check: macOS app shell (phase 11, branch macos/phase-11)

Date: 2026-10-14. Docs checked: codebase-summary, system-architecture, project-changelog, development-roadmap. No code edited, no new doc files.

## Verified against code

| Claim | Result |
|---|---|
| FsyncMac tests: 89 with 3 skipped | Confirmed by `swift test` run. Accurate for `FsyncMacCoreTests` only. The full target also has 5 `FsyncMacUITests` (all pass), so the total is 94 (89 run + 3 skipped) |
| FFI contract v2 in codebase-summary | Confirmed: `core/fsync-ffi/src/lib.rs` `contract_version()` returns 2 |
| Keychain fallback and migration | Confirmed: `KeychainStore.swift` (`useDataProtection` probe, `readMigrating`, `delete` clears both stores, process-wide recursive lock, add-only `loadOrCreateKey`) |
| SessionGate | Confirmed: opened at boot when a session is stored and after sign-in; closed on sign-out, `requireSignIn`, and failed boot |
| Status display | Confirmed: `StatusSummary` and `DeviceRowView` show Online for presence only; Connected only for live LAN/relay |
| Run instructions | Confirmed: `FSYNC_API`/`FSYNC_RELAY` override Info.plist keys; default `http://127.0.0.1:8080`; plain http only for localhost; Keychain service and data folder names; launch at login bundle-only |
| Script names and paths | Confirmed: `build-fsync-mac-app.sh`, `run-fsync-mac-integration.sh` exist under `apps/macos/scripts/` |
| Remove confirmation, recovery two-step, SAS bound to request id, trust-reset wiring, pasteboard markers | Confirmed in source |
| Integration run does not assert Lan | Confirmed: test asserts `online`, not `lan` |

## Changes made

| File | Change |
|---|---|
| `docs/codebase-summary.md` | Test table: FsyncMac 68 to 94 (89 Core, 5 UI, 3 skipped); FsyncCoreKit "6 functions" to 8 + 14. Build script usage adds `--version`. Notes `--bundle-id` only sets `CFBundleIdentifier`. Dev OTP needs `RUST_LOG=fsync_backend=debug`. Integration script needs swift and the xcframework. CI row notes that `app-macos` does not run the Swift tests. |
| `docs/system-architecture.md` | Header: phase 11 moved from "not built" to "built"; phases 12-16 listed as not built. FFI references "Contract v1" to v2 (two places). |

`docs/project-changelog.md` and `docs/development-roadmap.md` were accurate and are unchanged.

Line counts after edits: codebase-summary 87, system-architecture 212, changelog 145, roadmap 48. All under 800.

## Gaps and recommendations

1. `apps/macos/scripts/run-fsync-mac-integration.sh` header says the flow reaches "status Lan". The test does not assert Lan. Stale comment in code (not edited, per constraints).
2. The tester report's "89" is the Core bundle only. Add the 94 total to the phase 11 changelog verification note if one is added.
3. `docs/ffi-contract.md` title is "FFI contract v1" while its body says `contract_version()` returns 2. Not one of the four docs; consider retitling to v2.
4. `ci.yml` app-macos job still echoes a placeholder that says "added in phase 11". Wire the Swift tests and integration script in CI or track it in the roadmap.
5. The validator `.claude/scripts/validate-docs.cjs` does not exist in this repo (only under `~/.gemini`). Not run. A manual relative-link check on the four docs found no broken links.

## Unresolved questions

- Should the phase 11 changelog entry get a verification line with the 94-test total and the integration PASS?
- Is `--bundle-id` meant to isolate Keychain items per build? If so, the script and `AppConfig.bundleId` need a change (code, out of scope here).

**Status:** DONE_WITH_CONCERNS
**Summary:** Four docs checked against code and a fresh `swift test` run. Corrected FsyncMac test counts (94 total, 89 Core and 5 UI, 3 skipped), the FsyncCoreKit count, the stale phase 11 "not built" header, the FFI v1 references (now v2), and three run-instruction details. Keychain, SessionGate, status display, script names, and the v2 FFI version were already accurate. Docs impact: minor.
**Concerns/Blockers:** Stale "status Lan" comment in the integration script, an app-macos CI placeholder that does not run the Swift tests, and a missing docs validator script. Details in the gaps list above.
