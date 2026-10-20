# Windows e2e gate (phase 08) — 2026-10-20

- Sha: rig-int b5ebece7764631c0dcb587010bbd69c0365dc083 (phase 08 head 7fe3621)
- Runner: owner, waso interactive desktop, app signed in + paired
- Verdict: **PASS (shell scope)**; title-collision check deferred to phase 10 (by design)

| Check | Result |
|---|---|
| Ids NavDevices/NavHistory/NavFiles/NavPairing/NavSettings | PASS |
| Names Devices/History/Files/Pair a device/Settings (footer Settings Name+Id verified, unique) | PASS |
| pair-with-cli.ps1 | PASS 9/9 |
| clipboard-with-cli.ps1 | PASS 13/13 |
| No page title equals nav name | FAIL as written: Files (TransfersPage.xaml:10), Pair a device (PairingPage.xaml:8), Settings (SettingsPage.xaml:8) |

## Disposition of title check
- Plan assigns the title renames to phase 10 (phase-08 Key Insights "Page titles change (phase 10)"; phase-10 Key Insights titles; phase-10 Success Criteria re-checks "no title equals a nav item"). Page xaml files are phase-10 owns; fixing in 08 = owns violation.
- Gate criterion in phase 08 was inconsistent with that split → amended; check carried to phase 10 gate.
- Risk now: none for e2e (scripts locate nav by AutomationId).

## Unresolved
- None for 08. Phase 10 must rename to "Files and transfers", "Pair this PC", "Preferences" (spec titles, not the owner's suggested alternatives).
