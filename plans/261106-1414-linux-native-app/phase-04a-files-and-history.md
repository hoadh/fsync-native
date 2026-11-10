# Phase 04a — Files (eager inbox), retention, offers, history copy-back

<!-- red-team 2026-11-06 #14 --> Split from the former phase 04 (files/history/settings/hotkeys). This half is inside the MVP cut line.

## Context Links
- [plan.md](plan.md), [phase-03](phase-03-clipboard-backends.md), [phase-04b](phase-04b-settings-drag-drop-cli-hotkeys.md), [clipboard research](../reports/researcher-261106-1354-linux-clipboard-hotkeys-secrets.md) §5, [red team](reports/red-team-261106-1438-plan-review.md) #8 #14
- Parity: `docs/adr/013-windows-clipboard-app.md` d6 (eager <= 20 MiB, 180 s, inbox `<item>` hex/dash ids, reuse complete copy), d10 (copy-back), d13 (retention), d14 (auto-accept cap 1 GiB); `docs/adr/009-macos-lazy-paste.md` (20 MiB threshold)
- Core: `service/clips.rs` (`fetch_file_to`, `cancel`), `service/history_api.rs` (`history`, `pin`, `delete`, `clear`), `service/history_reps.rs` (`history_item_reps`)

## Overview
- Priority: P2. Status: In Progress (implemented; open: manual Omarchy file-manager paste, LIVE Nautilus). Estimate: 3 d.
- File clips received -> local files on the clipboard; pushed files (offers) with accept UI; history view with copy-back.

## Key Insights
- Linux has no virtual-file paste: always eager into `$data_dir/inbox/<item_id>/` then `text/uri-list` + `x-special/gnome-copied-files` (`copy\n` + URIs).
- <!-- red-team 2026-11-06 #14 --> Above 20 MiB total: no auto download and no notification action. Plain notification "Large file clip (N MiB) - open fsync to download"; History row shows a Download button -> fetch -> `xdg-open <dir>` (no `org.freedesktop.FileManager1`).
- Core keeps relative layout: uri-list lists top-level entries only (folders paste as folders).
- Pushed files (`on_incoming_files`) differ from file clips: go to `$XDG_DOWNLOAD_DIR/fsync`, accept prompt unless `auto_accept_files` and total <= 1 GiB.
- Copy-back: `history_item_reps(item_id)` gives full content -> write via backend (GNOME: QuickClipWindow from 03; the History window is already focused).
- <!-- red-team 2026-11-06 #8 --> Our own outgoing file entries include `x-special/gnome-copied-files`, so a second fsync instance treats them as file-manager copies (keeps L1 -> L2 e2e working under the 03 FM-type rule).

## Requirements
Functional:
- Inbox: item dir name validated (`^[0-9a-f-]{1,64}$`), file names from core validated again (no `/`, `..`, NUL, `.`/`..` refused); download timeout 180 s; newer file clip cancels older download; complete earlier copy reused.
- Retention: inbox follows history retention (days, then oldest-first until under `retention_bytes`), at start, every 24 h, on settings change; current clipboard item kept.
- Offers view: incoming pushes (from, count, size), Accept/Decline, progress from `on_transfer`, cancel; done -> notification + "Open folder" button in view (`xdg-open`).
- History view: list (kind icon, preview text, from, time), search, pin, delete, clear (keep pinned), Copy (copy-back), Download for large file clips.
- E2E command additions (feature `e2e-control`): `offer-accept`. <!-- red-team 2026-11-06 #6 -->
Non-functional: downloads off GUI and callback threads; UI progress <= 4 updates/s per transfer; files < 200 lines.

## Architecture
```
core callback (Files ClipItem) -> sync::inbound -> files::inbox::fetch_all(item) [own thread, cancel token]
     -> <= 20 MiB: clipboard write OwnedEntry{ text/uri-list, x-special/gnome-copied-files, marker, hint }
     -> >  20 MiB: plain notification; HistoryModel row Download -> fetch_all -> xdg-open dir
core callback (on_incoming_files) -> TransfersObject.offers -> Accept -> fetch_file_to(download dir) -> on_transfer progress
HistoryModel -> op thread: history(query, limit) ; Copy -> history_item_reps -> formats::to_entry -> backend.write
```

## Related Code Files
Create (under `/home/alex/Work/fsync-native/apps/linux/`):
- `platform/src/files/{mod.rs,inbox.rs,uri_list.rs,retention.rs}`
- `app/src/bridge/{history_model.rs,transfers_object.rs}`
- `app/qml/{HistoryView.qml,OffersView.qml}`
Modify: `platform/src/sync/inbound.rs` (files kind), `platform/src/clipboard/formats.rs` (uri-list + gnome-copied-files out), `platform/src/lib.rs`, `app/src/bridge/{mod.rs,engine_object.rs,delegate_adapter.rs,clipboard_object.rs}`, `app/qml/Main.qml`, `app/src/tray.rs` (History entry), `app/src/main.rs` (`offer-accept` test cmd), `tests/e2e/clipboard-with-cli.sh` (files, offers, history)

## Implementation Steps
1. `uri_list.rs` (RFC 2483 CRLF, percent-encoding, gnome-copied-files body) + tests.
2. `inbox.rs`: path building/validation, fetch loop over `FileInfo` with `fetch_file_to`, cancel/newest-wins, reuse complete copy (marker `.complete` written last), 180 s budget; tests with fake fetcher trait.
3. `retention.rs`: scan inbox dirs by mtime, keep current item id, apply days then bytes; tests.
4. Inbound files wiring + > 20 MiB plain notification + History Download. <!-- red-team 2026-11-06 #14 -->
5. `transfers_object.rs` + `OffersView.qml`: offers, accept/decline, progress, cancel, auto-accept rule.
6. `history_model.rs` + `HistoryView.qml` (search debounced 250 ms; copy-back; Download).
7. Extend e2e: CLI `send` -> offer accepted via IPC `offer-accept` -> file hash match; file clip L1 (wl-copy offering `text/uri-list` + `x-special/gnome-copied-files` of a temp file) -> L2 inbox -> L2 `wl-paste --type text/uri-list` lists inbox path, hash match; app history contains CLI text, copy-back -> `wl-paste` equals; > 20 MiB clip not auto-downloaded.

## Todo List
- [x] uri_list + gnome-copied-files + tests
- [x] inbox (validate, fetch, cancel, reuse) + tests
- [x] retention + tests
- [x] > 20 MiB notification + History Download
- [x] Offers/transfers UI with auto-accept cap
- [x] History model/view + copy-back
- [x] e2e extended, PASS

## Success Criteria
- `cargo test --manifest-path apps/linux/Cargo.toml --workspace` green; clippy clean.
- `apps/linux/tests/e2e/clipboard-with-cli.sh` PASS twice incl. file clip, push, history copy-back.
- Manual Omarchy: file clip from a second app instance pastes in a file manager (Thunar/Nautilus on host).
- `build-in-container.sh ubuntu-2204` smoke exit 0 (views load on Qt 6.2).

## Pending Live Verification
<!-- red-team 2026-11-06 #14 --> Dolphin/Thunar on other desktops -> ADR 014 "untested".
- [ ] LIVE: paste of received files in Nautilus on GNOME VM — evidence: ADR 014 table

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Nautilus ignores plain uri-list | M x M | Also offer `x-special/gnome-copied-files` |
| Inbox grows unbounded | L x M | Retention on start/daily/settings change |
| Path traversal via remote names | L x H | Validate item id + every component; canonicalize under inbox root |
| History copy-back on GNOME needs focus | M x L | Copy from the focused History window |

## Security Considerations
- Inbox and downloads 0700 dirs; file names validated; retention uses `symlink_metadata`, never follows symlinks.
- Auto-accept capped at 1 GiB; offers show sender device name.

## Rollback
- Additive modules; revert phase commits. Core schema unchanged.

## Next Steps
- Phase 04b (settings, drag-drop, CLI hotkeys) or, if cut, phase 05.
