---
owns: ["docs/design/desktop-ui-redesign-spec.md", "docs/adr/003-minimum-os.md"]
dependsOn: []
---

# Phase 01: Shared design spec (this Mac)

## Context Links
- Inventory + e2e name contract: `reports/explore-261019-ui-inventory-macos-windows.md`
- Research: `research/researcher-261019-macos-native-ui-patterns.md`, `research/researcher-261019-winui3-fluent-polish.md`
- `docs/adr/003-minimum-os.md`, `docs/adr/012-account-recovery-trust-reset.md`
- Copy sources: `apps/macos/Packages/FsyncCoreKit/Sources/TrustResetPrompt/TrustResetMapping.swift`, `TrustResetPendingView.swift`, `TrustResetOutcomeView.swift`, `apps/macos/FsyncMac/Sources/FsyncMacUI/Onboarding/RecoverySheet.swift`, all `apps/windows/src/Fsync.App/Views/*.xaml`, `apps/windows/tests/e2e/*.ps1`
- Sanitizer reference: `apps/macos/FsyncMac/Sources/FsyncMacCore/Features/Devices/PeerText.swift`

## Overview
- Priority: P1 (blocks every other phase). Status: pending.
- One compact spec that both tracks implement. It covers only what implementers would otherwise guess: the shared copy table, e2e-locked strings, the AutomationId list, the icon map, notification rules, the sanitizer spec and the state matrix. Docs only. <!-- red-team #15 -->
- Dropped from the earlier draft: `docs/design-guidelines.md` and long IA prose. The IA lives in plan.md Decisions and the shell phases. No new ADR. ADR 003 is amended only because of the macOS 14 bump. <!-- red-team #15 -->

## Key Insights
- Windows e2e (`apps/windows/tests/e2e/*.ps1`) finds controls by UIA Name. Locked strings (exact): `Email`, `Send code`, `Code`, `Sign in`, `Show a code`, `The words match`, `Paired`, nav `Devices` `History` `Files` `Settings`, `Remove`, dialog title `Remove {name}?`, `Copy`, `Save to Downloads`, Send-to picker button = device name, toggles `Start fsync when I sign in to Windows`, `Send my clipboard only when I ask (manual mode)`, `Download copied files before pasting (compatibility)`, `Save`, `Saved.`. The pairing code element name matches `^\d{6}$`. SAS words are one element. Device row Name = `DeviceItem.ToString()` = `{Name}, {Platform}, {Status}`.
- Page titles must not equal any nav item name. `Invoke-Name` uses `FindFirst(Descendants)`, so a duplicate name is a latent e2e break. Today two titles collide: `PairingPage.xaml` "Pair a device" and `SettingsPage.xaml` "Settings". New Windows titles: Devices "Your devices", History "Clipboard history", Files "Files and transfers", Pairing "Pair this PC", Settings "Preferences". Nav item names and e2e-locked control names stay unchanged. <!-- red-team #2 -->
- Sanitizer spec (single source for both platforms): strip Unicode general categories Cc (control) and Cf (format, incl. bidi overrides U+202A-202E, U+2066-2069, ZWJ/ZWNJ). Then trim whitespace. Then cap at 40 characters and append "…" when cut. Empty result → "another device". This matches the existing `PeerText.sanitize(_:limit:)` exactly. The spec includes test vectors that both platforms copy into their tests (newline, RLO, 41 chars, only-controls). <!-- red-team #10 -->

## Requirements
- Spec sections, in `docs/design/desktop-ui-redesign-spec.md` (< ~250 lines, tables only):
  1. Copy table: key | macOS | Windows | e2e-locked (Y/N). Every locked string from Key Insights is flagged Y. It includes trust-reset/recovery copy (from `TrustResetMapping.swift`, `RecoverySheet.swift`), empty states, notification title/body, and the page titles above.
  2. AutomationId list (Windows): NavDevices, NavHistory, NavFiles, NavPairing, NavSettings, StartingRing, BootFailedInfoBar, ResetHistoryButton, TrustTrustButton, TrustKeepButton, RecoveryLink.
  3. Icon map: concept | SF Symbol (macOS 14 name) | Segoe Fluent/MDL2 glyph (devices, history, transfers, pairing, status dots, offline, warning). Menubar icon-state glyphs are listed under "Stretch".
  4. Notification rules: incoming files only. Title `Files from {sender}`; body `{n} files, {size}` + `Saving to Downloads` (auto-accepted) or `Open fsync to accept`. Never file names. Sender goes through the sanitizer. Coalescing: at most one notice per sender per 10 s; later offers in that window update the count ("{k} offers from {sender}"). Arrival never activates the app or opens/focuses a window; only an explicit click does. Click → Transfers/Files page. Suppressed when that page is visible and the app is active. No sound. Windows toast arguments: fixed constant set (`open-files`), anything else ignored. A cold start with toast args only opens the window. <!-- red-team #10 -->
  5. Sanitizer spec + test vectors (Key Insights). <!-- red-team #10 -->
  6. State matrix: screen x {loading, empty, error, offline, populated} for both platforms (macOS has no Offline phase: banner only).
  7. Destructive dialog rule: default/Enter = Cancel/Close; destructive button never default; the dialog is bound to the id it opened with and closes if that id changes. <!-- red-team #11 -->
- ADR 003 amendment: macOS 14+ from 2026-10-19 (openSettings, ContentUnavailableView, symbolEffect). Windows floor unchanged. Consequence: macOS 13 users stay on the last 13-compatible build (release note in phase 12). No Liquid Glass mention beyond "automatic adoption only". <!-- red-team #6 #14 -->

## Architecture
Docs only. Data flow: spec → read by phases 02-11 (copy, ids, rules) → reconciled in phase 12 (implementers record deviations in their phase report; the spec owner applies them in a follow-up commit tagged phase 01).

## Related Code Files
- Create: `docs/design/desktop-ui-redesign-spec.md`
- Modify: `docs/adr/003-minimum-os.md`
- Delete: none

## Implementation Steps
1. Read the inventory, the research, all Windows XAML, the e2e scripts and the macOS copy sources.
2. Write spec sections 1-7. Grep each locked string from the e2e scripts and confirm it is in the copy table with Y.
3. Amend ADR 003 (status line + consequence).
4. Commit and push to `feat/desktop-ui-redesign` together with the plan dir (plan.md Remote loop step 0). <!-- red-team #3 -->

## Todo List
- [ ] Copy table with e2e-locked column + renamed page titles
- [ ] AutomationId list, icon map
- [ ] Notification rules + sanitizer spec + vectors
- [ ] State matrix, destructive dialog rule
- [ ] ADR 003 amendment
- [ ] Plan dir + spec committed and pushed

## Success Criteria
- Every locked string from the e2e scripts appears verbatim in the copy table (grep checklist in the commit message or the phase report).
- No page title in the copy table equals a nav item name.
- `git diff --name-only main` lists only the two owned files plus the plan dir.
- Pushed to origin before any waso dispatch.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Spec drifts from implementation | M x M | Deviations noted in phase reports; reconciled in a phase 01 follow-up commit |
| Locked string missed | L x H | Grep checklist; e2e gate after phase 08 |
| Over-specification | M x L | Spec limited to the 7 sections above |

## Security Considerations
- Forbids file names / clipboard content in notifications, tooltips, logs. One sanitizer for all peer-supplied text.
- Trust-reset and recovery copy stays explicit about consequences (ADR 012).

## Next Steps
- Unblocks 02 (local) and 07 (waso) in parallel. Rollback: revert the docs commit; no code depends on it at runtime.
