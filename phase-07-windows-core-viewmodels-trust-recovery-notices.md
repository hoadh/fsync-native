---
machine: waso
owns: ["apps/windows/src/Fsync.App.Core/ViewModels/TrustResetPromptViewModel.cs", "apps/windows/src/Fsync.App.Core/ViewModels/TrustResetMessages.cs", "apps/windows/src/Fsync.App.Core/ViewModels/AccountRecoveryViewModel.cs", "apps/windows/src/Fsync.App.Core/ViewModels/IncomingFilesNotice.cs", "apps/windows/src/Fsync.App.Core/ViewModels/IncomingFilesNoticeCoalescer.cs", "apps/windows/src/Fsync.App.Core/ViewModels/ToastAction.cs", "apps/windows/src/Fsync.App.Core/Services/PeerText.cs", "apps/windows/src/Fsync.App.Core/Bridge/EngineBridge.Trust.cs", "apps/windows/src/Fsync.App.Core/Files/TransfersViewModel.cs", "apps/windows/tests/Fsync.App.Tests/FakeEngine.cs", "apps/windows/tests/Fsync.App.Tests/TrustResetPromptViewModelTests.cs", "apps/windows/tests/Fsync.App.Tests/AccountRecoveryViewModelTests.cs", "apps/windows/tests/Fsync.App.Tests/IncomingFilesNoticeTests.cs", "apps/windows/tests/Fsync.App.Tests/PeerTextTests.cs", "apps/windows/tests/Fsync.App.Tests/ToastActionTests.cs", "apps/windows/tests/Fsync.App.Tests/EngineRecreateViewModelTests.cs"]
dependsOn: [01]
status: in-progress
reviewedSha: 76f001cc64eb741b6ede2b824b47569ddb5ec396
reviewScore: 10
---

# Phase 07: Windows core view models: trust reset, recovery, notices (waso)
<!-- red-team #15: owns narrowed from Fsync.App.Core/** to exact files -->

## Context Links
- Spec: `docs/design/desktop-ui-redesign-spec.md` (copy, sanitizer + vectors, notification rules, destructive dialog rule)
- macOS semantics: `apps/macos/Packages/FsyncCoreKit/Sources/TrustResetPrompt/{TrustResetPromptModel,TrustResetMapping}.swift`, `apps/macos/FsyncMac/Sources/FsyncMacCore/Features/Onboarding/AccountRecoveryModel.swift`, `.../Features/Devices/PeerText.swift`, tests `TrustResetPromptModelTests.swift`, `AccountRecoveryModelTests.swift`
- ADR 012, `docs/ffi-contract.md`
- Windows: `apps/windows/src/Fsync.App.Core/Bridge/EngineBridge.cs` (`AccountEventReceived` line 50, `ResetHistoryAsync` line 134), `EngineBridge.Clips.cs` (`IncomingFiles`), `Files/TransfersViewModel.cs` (`OnIncomingFiles` line 63), tests `FakeEngine.cs`, `ViewModelTests.cs`

## Prerequisites (orchestrator, before dispatch; plan.md Remote loop step 0) <!-- red-team #3 -->
1. Plan dir + phase 01 output committed and pushed on `feat/desktop-ui-redesign`; `R doctor waso` passes.
2. DLL: the owner copies `fsync_ffi.dll` into `apps/windows/src/Fsync.CoreBindings/runtimes/win-x64/native/` (gitignored) of the worker clone AND the `-rig-test` clone, and records the sha256 of both in the baseline report. If skipped, the owner explicitly accepts the failing-test list as the baseline.
3. Baseline: `R run waso --ref <base-sha> -- '<win cmd>'` at the commit phase 07 will be dispatched from. Write `reports/windows-test-baseline-<date>.md` (sha, DLL sha256 or "absent, accepted", exact failing test names). Gate for every waso phase: no failures outside this list.

## Overview
- Priority: P1. Status: pending.
- Headless logic (no WinUI) for the parity items: `TrustResetPromptViewModel`, `AccountRecoveryViewModel`, `IncomingFilesNotice` + coalescer, `PeerText.Sanitize`, `ToastAction`, bridge `ApplyAccountState`, and the auto-accept decision exposed by `TransfersViewModel`. xUnit on `FakeEngine`. FFI unchanged.
- Stretch items (IsFirstDevice, pairing countdown) moved to phase 11. <!-- red-team #6 -->

## Key Insights
- `AccountEvent.TrustReset(resetId, devices, deviceNames)` → `EngineBridge.HandleAccountEvent` → `AccountEventReceived`. This event is declared on the bridge (EngineBridge.cs:50), not on the engine, so a VM subscription survives the `ResetHistoryAsync` engine recreate. Rule: VMs hold the bridge, never an `IFsyncEngine`; after the recreate they refresh on the Ready transition. A test proves events from the new engine reach the VM. <!-- red-team #9 -->
- Confirm returns `AccountState`: `Ready`/`Created` → trusted; `NeedsPairing` → removed. The bridge needs `ApplyAccountState(AccountState)` in a new partial `EngineBridge.Trust.cs` (EngineBridge.cs is 189 lines).
- Mirror Swift semantics: confirm/decline use the reset id the user saw; `NotFound` → refresh + notice; shown id changes → notice "The device list changed. Review it again."; busy guard; stale errors dropped.
- Sequence token <!-- red-team #9 -->: every `Refresh()` takes `++_seq`; a result whose seq != `_seq` is dropped (overlapping refreshes from event + Ready transition).
- Dialog binding <!-- red-team #11 -->: `BeginConfirm(resetId)` records `ConfirmingResetId`. If `Current?.ResetId` changes while it is set, the VM clears it, raises `ConfirmAborted`, and sets the list-changed notice; the page (phase 09) hides the dialog. `Confirm(resetId)` with an id other than `ConfirmingResetId` is a no-op.
- `DeviceItem.ToString()` = UIA Name of device rows: do NOT change it.
- Sanitizer <!-- red-team #10 #11 -->: `PeerText.Sanitize(string?, int limit = 40)` is a port of the macOS spec (strip `UnicodeCategory.Control` and `UnicodeCategory.Format`, trim, cap + "…", empty → "another device"). It is used for notice senders and `TrustResetInfo.deviceNames` (exposed as `DisplayNames`). Tests use the spec vectors.
- Auto-accept decision <!-- red-team #10 -->: `TransfersViewModel.OnIncomingFiles` raises `public event Action<ClipItem, bool>? OfferReceived` (offer, autoAccepted) after its existing decision, and only for an itemId not seen before (bounded set). The notice builder never re-derives the rule.
- Coalescer: per sanitized sender, at most one notice per 10 s; later offers bump the count. Uses `TimeProvider` (built into .NET 8, no package).
- `ToastAction`: `enum { OpenFiles }`, `TryParse(string? args, out ToastAction)` accepts only the exact constant `action=open-files`; everything else → false. <!-- red-team #10 -->
- Real-engine tests need the DLL; new tests use `FakeEngine` only.

## Requirements
- `TrustResetPromptViewModel` (ObservableObject, IDisposable):
  - `Pending`, `Current`, `HasPending`, `IsBusy`, `Outcome`, `Message`, `MessageIsNotice`, `DisplayNames`;
  - `Refresh()` (seq-guarded), `BeginConfirm(id)`, `CancelConfirm()`, `ConfirmingResetId`, `event ConfirmAborted`, `ConfirmCommand(id)`, `DeclineCommand(id)`, `DismissOutcome()`;
  - subscribes to `AccountEventReceived` + bridge `PropertyChanged(Phase)` (→ Refresh on Ready); Dispose unsubscribes both.
- `TrustResetMessages`: the 4 mapped messages + the list-changed notice (spec copy).
- `AccountRecoveryViewModel`: `Step` (Idle/Explain/FinalConfirm), `IsBusy`, `Error`, `Begin`, `Proceed`, `Cancel`, `ResetAsync` (no-op unless Step == FinalConfirm); `ReauthRequired`/`Auth` → `RequireSignIn()`.
- `EngineBridge.Trust.cs`: `ApplyAccountState`, `RequireSignIn` (reuse the SignedOut path).
- `IncomingFilesNotice.For(ClipItem offer, bool autoAccepted, int offerCount)` → Title/Body per spec; `ShouldShow(bool windowActive, bool filesPageVisible)`.
- `IncomingFilesNoticeCoalescer`, `ToastAction`, `PeerText`, `TransfersViewModel.OfferReceived`.
- Non-functional: files < 200 lines; UI-thread affinity like the other VMs.

## Architecture
```
Rust → EngineDelegateAdapter → ui.Post → EngineBridge.HandleAccountEvent → AccountEventReceived (bridge-level)
   → TrustResetPromptViewModel → Refresh(seq) → RunAsync(PendingTrustResets) → drop if stale
Bridge.Phase → Ready → Refresh (covers ResetHistoryAsync engine recreate)
UI BeginConfirm(id) → dialog → Confirm(id) → RunAsync(ConfirmTrustReset) → Outcome + ApplyAccountState
OnIncomingFiles → decision → OfferReceived(offer, auto) → [App, phase 09] coalescer → IncomingFilesNotice → toast
```

## Related Code Files
- Create: the VM/notice/toast/PeerText/Trust partial files + 6 test files in frontmatter
- Modify: `apps/windows/src/Fsync.App.Core/Files/TransfersViewModel.cs` (additive event + seen-id set), `apps/windows/tests/Fsync.App.Tests/FakeEngine.cs` (scriptable trust/recovery/account-state calls; engine factory can return a fresh instance)
- Delete: none

## Implementation Steps
1. Port the Swift trust/recovery tests 1:1 first, then add: stale seq dropped, Ready-transition Refresh, ConfirmAborted on id change, stale-id Confirm no-op.
2. FakeEngine scriptable.
3. Implement the VMs + messages + Trust partial.
4. `EngineRecreateViewModelTests`: bridge with FakeEngine factory → ResetHistoryAsync → TrustReset event from the new engine reaches the VM; Ready → Refresh called once.
5. PeerText + vectors; notice + coalescer (fake TimeProvider); ToastAction; OfferReceived (auto/manual/duplicate).
6. Phase test command; compare failures to the baseline list.

## Todo List
- [ ] FakeEngine scriptable
- [ ] TrustResetPromptViewModel (seq, Ready refresh, confirm binding) + tests
- [ ] AccountRecoveryViewModel + EngineBridge.Trust + tests
- [ ] Engine recreate tests
- [ ] PeerText, IncomingFilesNotice, coalescer, ToastAction + tests
- [ ] TransfersViewModel.OfferReceived + tests

## Success Criteria
- waso: `dotnet build apps/windows/src/Fsync.App -c Release -p:Platform=x64 -r win-x64 && dotnet test apps/windows/tests/Fsync.App.Tests -c Release`, with no failures outside `reports/windows-test-baseline-<date>.md`. All new tests pass.
- No file outside `owns`; no csproj/props/package change; `DeviceItem.ToString()` unchanged.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Baseline unknown → regressions hidden | M x H | Step-0 baseline report; gate by exact list |
| Semantics drift from macOS | M x H | Port Swift tests 1:1 |
| Overlapping refresh shows stale list | M x M | Sequence token + test |
| VM lost after engine recreate | M x H | Bridge-level events + Ready refresh + test |

## Security Considerations
- Trust confirm acts only on the id the dialog opened with; no auto-confirm. Peer text sanitized. Messages via `UserMessages`; no device ids/file names in `AppLog`.

## Next Steps
- Remote loop: review → supply-chain gate → `review --sha` → `collect --test` → dispatch 08 on `rig-int/<slug>`. <!-- red-team #1 --> Rollback: revert the collected merge on rig-int via a resume-round revert commit; VMs are unused elsewhere.
