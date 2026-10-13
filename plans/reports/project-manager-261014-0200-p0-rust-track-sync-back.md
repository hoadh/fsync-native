# P0 Rust track: plan sync-back (fsync-native MVP)

Date: 2026-10-14. Scope: phases 01, 02, 06, 07, 08, 09 (Mac). Phase 03 plan row only. 04, 05, 10-16, procurement: not synced.
Method: read plan files, the 261013/261014 reports, and checked repo files by path/grep. No builds or tests run. No git repo, so no commit history checked.

## Status changes

| Phase | Before | After | Todos ticked | Why not Complete |
|---|---|---|---|---|
| 01 | pending | Partial | 1/5 | ADR sign-off, procurement, CI 3-OS green unverified (no git) |
| 02 | pending | Partial | 4/6 | Two-machine LAN, NAT hole punch, mDNS fallback not decided |
| 03 | pending | Partial | plan row only | Swift done (UniFFI 0.29.4, ADR 008); C# open |
| 06 | pending | Partial | 4/5 | DeviceList branch coverage unmeasured; two-machine pairing |
| 07 | pending | Partial | 6/6 | LAN throughput vs 80% link, LAN/relay latency targets, two machines |
| 08 | pending | Partial | 6/8 | CI bindings artifacts missing; P0 exit demo one host only |
| 09 | pending | Partial | 7/7 | 10k WS load test not run; no real email provider |

Totals: 28/37 scoped todos ticked. Complete: 0/16.

Files: `plan.md` (73 lines, status table, progress block), new `validation-log.md` (Validation Log moved verbatim from plan.md to get under 80 lines), phase-01/02/06/07/08/09 (status, ticked todos, Implementation Notes section). phase-03 file not edited (out of scope).

## Blockers and risks

1. **P0 exit gate not proven on the final tree (high).** `plans/reports/tester-261014-0130-p0-rust-track-final-test-run.md` is named "final" but is INTERIM. Its e2e relay-only round failed on snap4. Round-5 review says a later e2e passed, but no tester report covers the final tree. Round-7 (`fullstack-developer-261013-2314-core-phase-08b.md`) ran fmt, clippy and `cargo test -p fsync-core` only. e2e and Swift were not run. Owner: tester (fresh agent). Done when: fmt, clippy `-D warnings`, `cargo test --workspace --no-fail-fast`, xcframework + swift test, spike build, and e2e x3 all pass on a frozen copy, with latencies recorded.
2. **Review score is reading-only (medium).** Round-6 review 9.6 "auto-approved" is from reading code. Gate claims are the coordinator's, not reproduced. Do not treat as verified.
3. **Two-machine evidence missing (high, hardware).** LAN discovery (mDNS median 2.7 s vs 2 s target), NAT hole punch, LAN throughput, mac-to-Windows P0 exit demo. Needs a second machine. Owner: user.
4. **Swift 20 ms gate open (medium).** 10 MiB roundtrip 37.8 ms in debug. Not re-measured in release. The roundtrip test was replaced by a typed-error test, so the gate is not in the suite. Owner: C (macOS).
5. **Flaky test (medium).** `backend/tests/devices_reset.rs` hit `PortNotExposed` once. Root cause not found. `pg.rs` retry is not selective (review L3). Owner: B.
6. **History key-loss path (medium).** Recovery needs `reset_history`, but no app UI or API confirms it. M5 TrustReset confirm is not in service or FFI. Owner: A (API), C/D (UI, phases 11/13).
7. **Pairing chain pruning (medium).** Backend keeps 100 rows per epoch, so a fresh device cannot always get a chain from genesis. Checkpoint mechanism not built. Owner: A + B.
8. **Repo hygiene (low).** No `.git`. `.gitignore` has only `/target`, so `spikes/iroh-discovery/target/` is not ignored. Owner: user (decide on git init).

## Deviations logged (scope changes)

- file/1 uses own blake3 chunk-group protocol, not iroh-blobs (ADR 007 addendum). Reason: CV outboard fit. Impact: phase 07 spec change.
- History wrong key no longer auto-resets. Keycheck returns `HistoryKeyMismatch`, nothing moved. Explicit `reset_history` recovers. Reason: data-loss review R3-1. Impact: phase 08 spec change.
- Re-pair after reset via `listed_head`, skips publish when node already listed (round 7). Reason: `DuplicateDevice` dead-end (review M1). SAS and MAC unchanged. Impact: phase 06 scope addition.
- SignedList carries head only, not full chain. Impact: root unknown until chain seen.
- rusqlite 0.32 not 0.40 (sqlx link conflict). Impact: ADR 011 addendum.
- Backend: no `AuthProvider` trait, in-memory rate limiter, JWT hand-rolled. Impact: phase 09.
- Deferred by agreement: M5 TrustReset confirm API, mdns-sd fallback.

## Next actions (owner, definition of done)

1. **Main agent: finish the plan.** Gate run (blocker 1) and the items below are required before P0 can be called done. Do not start P1 app work on an unverified core.
2. **Tester:** full gate run on a frozen tree. DoD: green list in blocker 1, latencies in a report named `tester-261014-HHMM-p0-final-gates.md`.
3. **Dev A (core):** M5 TrustReset API and history-reset surface in FFI. Chain checkpoint for pruned history. DoD: tests for both; FFI contract updated.
4. **Dev A/B:** CI job that builds and publishes xcframework and bindings artifacts (phase 08 todo). DoD: job green on main.
5. **Dev B:** replace `pg.rs` retry with a selective wait, fix `devices_reset` flake. DoD: 5 consecutive green runs.
6. **Dev B:** 10k idle WS load test, Postgres TLS (`tls-rustls`). DoD: numbers recorded in phase 09.
7. **Dev C:** release-build Swift roundtrip timing against 20 ms gate. DoD: number in phase 08 notes.
8. **User:** second Mac/PC for two-machine runs (phase 02, 06, 07, 08). Procurement and signing (phase 01). ADR 001-006 sign-off. Email provider choice (plan Q3). DoD: each item closed or explicitly deferred in plan.md.
9. **User:** decide git init and add `spikes/**/target` to `.gitignore`. DoD: repo under VCS, CI runnable.

## Unresolved questions

1. Who runs the final frozen-tree gate, and when? (Only interim results exist.)
2. Is the repo meant to be under git now? ADR "merged" and CI green cannot be checked without it.
3. Is the Swift 20 ms roundtrip a release gate, or is the path/chunk design sufficient?
4. Who owns the second machine for two-machine runs, and when?
5. Is M5 TrustReset confirm a P0 follow-up or part of phase 11/13?
6. Email provider (SES / Resend / Postmark): still open. Blocks real OTP delivery.
7. Root cause of the `devices_reset` `PortNotExposed` failure: testcontainers race or regression?
8. Should the chain checkpoint live in phase 06 (core) or phase 09 (backend)?

**Status:** DONE_WITH_CONCERNS
**Summary:** Synced phases 01, 02, 06-09 and phase 03 row to Partial, with 28/37 scoped todos ticked and 0/16 Complete. Moved the Validation Log out of plan.md so it is 73 lines. Logged all deviations.
**Concerns:** P0 exit gate is not proven on the final tree (last full run was interim with a red e2e). Two-machine evidence is missing. The Windows/C# half and procurement are open. Bash and the `ck` CLI were not available in this session, so plan edits were made directly, which the fallback rule allows.
