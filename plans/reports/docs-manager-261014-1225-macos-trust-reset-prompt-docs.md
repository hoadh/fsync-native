# Docs check: macOS TrustResetPrompt library

**Docs impact:** minor

## Verified (no edits needed)
- docs/ffi-contract.md (~131-133): `TrustResetPromptModel(engine:)`, `refresh()`, `handle(_:)`, `resetId` answers, main-actor hop. Matches code.
- docs/project-changelog.md (11-12): outcomes, NotFound refresh, `Text(verbatim:)`, no reset code, second confirmation, `TrustResetPromptTests`, `TRUST_RESET_SNAPSHOT_DIR`, macOS 13. Matches code.
- docs/codebase-summary.md (16): library and tests exist in `FsyncCoreKit/Package.swift`. Matches code.

## Edited
- docs/development-roadmap.md (item 3): removed "macOS app still has to show the warning"; library now shows it, app wiring is open (phase 11).
- docs/system-architecture.md (line 11): added `TrustResetPrompt` library to the macOS app line.

## Unresolved
- docs/codebase-summary.md line 11 says `fsync-ffi` is "contract v1"; `contract_version()` returns 2 (core/fsync-ffi/src/lib.rs:29). Not edited (outside this task's scope).
