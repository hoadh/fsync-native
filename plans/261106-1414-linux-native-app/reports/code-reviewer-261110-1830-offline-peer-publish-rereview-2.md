# Re-review 2: offline peer publish, fix round (90d3b47..749919c)

Score **9.5/10**. Critical 0, high 0, medium 0, low 6. **APPROVED** (bar 9.5 met). Two conditions: the macOS CI job must compile and pass, and L-new-1 (old binding against a new library) should be accepted knowingly.

Scope: `git diff 90d3b47..HEAD` (8ae0021, f59ee84, 02c31de, fe4a80c, f820439, 749919c). I also checked the net change `ac0f4eb..HEAD` for anything that touches the new code. Each previous finding was re-checked against the code, not against the implementer's claims.

## Previous findings, re-verified
| ID | Verdict | Evidence |
|---|---|---|
| M1 FFI loses pending / Windows misreport | FIXED | `fsync_ffi::Published.pending` is added last and `From<svc::Published>` maps it. Windows `DescribePublished` and macOS `publishMessage` call a pending device "not confirmed"; Windows shows "no other device is connected" only when delivered and pending are both 0. ffi-contract.md now covers Semantics and "Additive since v2". |
| M2 Linux "Sending..." never resolves | FIXED | The line is now static: "Sent to 1 device; 1 not confirmed." or "Not confirmed by 2 devices.", and failures are appended. The contract says late results are only logged. |
| M3 latest-wins unproven + always-true assert | FIXED | New `slow_live_peer_gets_the_newest_clip_never_an_older_one_after_it` asserts exactly `[n, n+2]`, then 2.5 s of silence. **Mutation re-run by me:** with the `latest == seq` check removed (in a scratch copy), the test FAILS. The returning-peer test now keeps listening until the away attempt has surely ended (away + 8 s), so its order assert is real. |
| L1 concurrent-publish ticket interleave | Open (noted) | Unchanged. All apps serialize their publishes. |
| L2 queued frames held for up to 7 s | Open (noted) | Unchanged. Bounded. |
| L3 panics swallowed | FIXED | `log_panic` runs on reap and on close; aborts are not logged. |
| L4 tight wall-clock margins | FIXED (mostly) | QUICK is now 3 s and NO_WAIT 1.25 s. The rate-limit test is unchanged (60 sequential publishes in under about 2 s). |
| L5 weak close test | FIXED | `send_timeout` is 30 s, so close finishing in under 10 s proves the abort. Publish after close asserts `failed == [b]`. |
| L6 "5 s per-peer timeout" docs | FIXED | protocol-spec and system-architecture now say 2 s + 5 s retry, 7 s at most. |

## Focus checks
- **Bindings, no hand edits:**
  - Swift: I regenerated it myself (release `libfsync_ffi.so` plus the repo's `uniffi-bindgen`, library mode, the same steps as `gen-bindings.sh`). The output is byte-identical to the checked-in `fsync_ffi.swift`.
  - C#: not regenerated here. I relied on the waso-vm evidence (generation diff identical, 181/181 passed). The C# hunk has the normal generator shape: positional record, Read/AllocationSize/Write in field order.
  - Record field changes do not alter uniffi function checksums, so the absence of checksum drift is expected.
- **ffi-contract backward compatibility:** it is correct that the Swift labeled initializer gains a trailing `pending:` and that C# hosts must regenerate. See L-new-1.
- **Swift correctness (read only, not compiled):**
  - `FsyncCoreKit.Published` qualification is valid: the module has no type named `FsyncCoreKit`, and the same pattern already compiles in `StorageSettingsView.swift` (`FsyncCoreKit.Settings`). It avoids the clash with Combine's `@Published`, which `ClipboardSync.swift` imports.
  - `static func` on a `@MainActor` class, called from a `@MainActor` test through `@testable import`: OK.
  - The nested func with implicit return, and the ternary continued on a line starting with `:`, are both valid Swift.
  - The test's initializer argument order (itemId, echo, delivered, failed, refused, pending) matches the generated init.
  - No other Swift or C# code constructs `Published`.
- **Windows:** `DescribePublished` is pure and covered by 8 cases including the all-pending case. Its wording matches Linux, minus the full stop (existing Windows style).
- **`clip_accept_delay` cannot reach release builds:**
  - The field and its use are `#[cfg(any(test, feature = "test-hooks"))]`.
  - `test-hooks` is enabled only by fsync-core's dev-dependency on itself. Resolver 3 does not unify dev-dependency features into normal builds.
  - `cargo tree -e features` for fsync-ffi and for fsync-linux shows 0 hits for `test-hooks`.
  - Linux `release-guard.sh` is an allowlist (`SHIPPED_FEATURES` is empty) over the normal and build dependency graph, plus an explicit `test-hooks` grep. `BUILT_FEATURES` would report the feature through `--build-info`.
  - The Windows (`build-windows-dll.ps1`) and xcframework scripts also refuse `test-hooks`.
- **Latest-wins test flakiness:**
  - The receiver delays 1 s, inside a 2 s first attempt, so there is 1 s of slack before a retry. Even a retry would not emit a duplicate, because offers are deduplicated by item_id.
  - Passed 5 times in a row and 6 times in parallel under load (about 8.1 s each).

## Low (remaining)
- **L-new-1:** appending a field to a positional record means an old binding against a new library fails at the first `publishClip` decode (uniffi's leftover-bytes check), with no load-time signal and no `contract_version` bump. This is documented and follows the `Published.refused` precedent. It is fine only because bindings always ship with the library.
- **L-new-2:** a refused peer is not mentioned when anything was delivered or is pending, on Linux, Windows and macOS (largely pre-existing). Example: 0 delivered, 1 pending, 1 refused gives just "Not confirmed by 1 device."
- **L-new-3:** `a_panicking_send_is_reaped_and_does_not_poison_the_set` is near-vacuous. `spawn` returns true whether or not the panicked task was reaped, and the log line is not asserted.
- **L-new-4:** `gen-bindings.sh` hardcodes `.dylib`, so it cannot be re-run on Linux as is (a `uname` switch would fix it).
- **L1, L2:** as in the table above (documented, not changed).

## Tests run (this host)
- `clip_offline_peer_publish`: 5/5 tests, 5 sequential runs and 6 parallel runs, all passed.
- fsync-core `clip::` unit tests: 4/4 passed. fsync-ffi tests: all passed.
- Mutation (latest-wins check removed, scratch copy): the slow-peer test FAILS as expected.
- Swift regeneration: identical to the checked-in file.
- apps/linux `status_line` unit test: passed.
- Not re-run: the full root workspace (coordinator evidence: 377 passed) and Windows (waso-vm evidence: 181/181). No macOS run. No Wayland/X11 tests.

## Unresolved questions
- The macOS compile and XCTest run is still pending. Approval assumes the CI `macos` job passes.
- Is the old-binding/new-library failure (L-new-1) acceptable without a `contract_version` bump, given that bindings always ship with the library?
- Should a refused peer be mentioned alongside pending or delivered (L-new-2)?

**Status:** DONE_WITH_CONCERNS (macOS unverified). **Score:** 9.5/10. **Approved:** yes, conditional on macOS CI.
