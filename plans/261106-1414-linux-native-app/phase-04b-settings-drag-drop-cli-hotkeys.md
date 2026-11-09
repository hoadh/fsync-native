# Phase 04b — Settings, drag-drop send, CLI hotkey binds

<!-- red-team 2026-11-06 #14 --> Split from the former phase 04. Below the MVP cut line: may be deferred; then SettingsView stays the 02/03 stub (autostart + clip mode) and 05 follows 04a.

## Context Links
- [plan.md](plan.md), [phase-04a](phase-04a-files-and-history.md), [phase-02](phase-02-engine-bridge-and-app-shell.md) (IPC `send`, `--send`), [red team](reports/red-team-261106-1438-plan-review.md) #8 #14
- Parity: `docs/adr/013-windows-clipboard-app.md` d8 (no Ctrl+Alt defaults, AltGr), d9 (send paths validation)
- Core: `service/clips.rs` (`send_files`), `service/history_api.rs` (`settings`, `set_settings`), `settings.rs`

## Overview
- Priority: P3. Status: In Progress (code + automated checks done; LIVE item open). Estimate: 2 d.
- Full Settings view; drag-drop and `--send` to a device; documented CLI hotkey binds per desktop. No portal GlobalShortcuts (cut).

## Key Insights
- <!-- red-team 2026-11-06 #14 --> Hotkeys = IPC commands only (`bind = SUPER ALT, C, exec, fsync-linux --clip-send` on Hyprland; Sway `bindsym`; GNOME/KDE custom shortcut running the command). The launcher passes `XDG_ACTIVATION_TOKEN` on GNOME (03 QuickClipWindow). ashpd GlobalShortcuts portal dropped (unverified support, extra runtime).
- Never Ctrl+Alt defaults (AltGr, ADR 013 d8).

- Settings help text (from 04a, ADR 014): retention days/size also apply to the file inbox; large clips the user downloaded explicitly are age-limited only (not counted in the size budget). The settings screen must call `inbox_runtime::retention_soon` after a change.

## Requirements
Functional:
- Settings view: clip mode Auto/Manual, retention days/size, max clip size, auto-accept files, denylist editor (note: X11 process names, Wayland app ids on Hyprland/Sway; hints elsewhere), autostart, backend status + detection reason + filter capability, secrets mode warning, shortcut help (copyable bind lines for Hyprland/Sway/KDE/GNOME). Writes via `set_settings` (validated by core).
- Drag-drop: drop files on a device row (or window -> DevicePicker) -> `send_files(node, paths)`; only `file://` local URLs, absolute, existing, not under the 03 path denylist. `--send <paths>` via IPC opens DevicePicker. <!-- red-team 2026-11-06 #8 -->
- E2E command addition (feature `e2e-control`): `pick-device`. <!-- red-team 2026-11-06 #6 -->
Non-functional: files < 200 lines; sends off GUI thread.

## Architecture
```
Settings QML <-> SettingsObject <-> op thread: settings()/set_settings()
DropArea / IPC send -> DevicePicker -> path checks (abs, exists, path_denylist) -> op thread: send_files -> on_transfer
```

## Related Code Files
Create (under `/home/alex/Work/fsync-native/apps/linux/`):
- `app/src/bridge/settings_object.rs`
- `app/qml/{SettingsView.qml,DevicePicker.qml,ShortcutHelp.qml}`
Modify: `app/src/bridge/{mod.rs,devices_model.rs}`, `app/src/main.rs` (`--send` -> picker, `pick-device` test cmd), `app/qml/{Main.qml,DevicesView.qml}` (DropArea), `app/src/tray.rs` (Settings entry), `tests/e2e/clipboard-with-cli.sh` (send), `apps/linux/README.md` (shortcut binds)
Delete: none (stub Settings toggle from 02 replaced inside `SettingsView.qml`).

## Implementation Steps
1. `settings_object.rs` + `SettingsView.qml` (+ autostart from 02, secrets warning, backend/filter status).
2. `ShortcutHelp.qml`: bind snippets per desktop with copy buttons; README section with the same lines.
3. DropArea on device rows + DevicePicker; `--send` IPC -> picker; path denylist reused from `platform::clipboard::path_denylist`.
4. Extend e2e: `fsync-linux --send <tmpfile>` + IPC `pick-device` -> CLI `fetch` hash match; `--send ~/.ssh/<file>` refused.

## Todo List
- [x] Settings object/view
- [x] Shortcut help + README binds
- [x] Drag-drop + `--send` DevicePicker with denylist
- [x] e2e send cases PASS

## Success Criteria
- `cargo test --manifest-path apps/linux/Cargo.toml --workspace` green; clippy clean.
- `apps/linux/tests/e2e/clipboard-with-cli.sh` PASS twice incl. `--send` cases.
- Manual Omarchy: Hyprland bind `SUPER ALT, C` triggers send in manual mode; settings change persists across restart.
- `build-in-container.sh ubuntu-2204` smoke exit 0.

## Pending Live Verification
- [x] LIVE: GNOME VM custom shortcut -> `fsync-linux --clip-send` focuses QuickClipWindow — evidence: ADR 014 table (GNOME 50, 2026-11-07: Super+Alt+C, auto-close without a click)

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Users expect in-app hotkey binding | M x L | Copyable binds; portal revisit listed in ADR 014 "later" |
| DropArea URL formats differ per file manager | L x M | Accept only `file://`; tests with Thunar/Nautilus urls |

## Security Considerations
- `--send` and drop paths re-validated in the running instance (abs, existing, denylist).
- Settings writes go through core validation only.

## Rollback
- Additive; revert phase commits; Settings stub from 02 remains usable.

## Next Steps
- Phase 05 packages the app.
