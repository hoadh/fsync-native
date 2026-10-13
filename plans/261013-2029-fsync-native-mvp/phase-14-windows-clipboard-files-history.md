# Phase 14 — Windows app: clipboard sync, file send, virtual-file paste, history, settings

## Context Links
- Design §4 flows 3-6; ADR 010 ([phase-05](phase-05-spike-windows-clipboard-lazy-paste.md)); [phase-07](phase-07-core-protocols-clip-file.md), [phase-08](phase-08-core-history-ffi-cli.md)

## Overview
- Priority: P1 | Status: pending | Effort: 5-6 wk (D) — parallel with phase 12
- Productionize spike: listener + filters, auto/manual, format mapping, delayed-render writer, virtual-file paste (or eager per ADR 010), drag-drop + Explorer "Send to", history, settings.

## Key Insights
- Format map: `CF_UNICODETEXT`↔`text/plain`, `HTML Format`(CF_HTML header wrap/unwrap)↔`text/html`, `Rich Text Format`↔`text/rtf`, `PNG`/`CF_DIBV5`→`image/png`, `CF_HDROP`→files; incoming files → `FILEDESCRIPTORW`/`FILECONTENTS` (lazy) or `CF_HDROP` (eager).
- Large non-file reps (big image) use delayed rendering (`WM_RENDERFORMAT` / IDataObject GetData → `fetch_rep`).
- Explorer "Send to" = shortcut in `shell:sendto` invoking `Fsync.exe --send <paths>` → IPC to running instance (AppInstance redirection).

## Requirements
- Same functional set as phase 12 (parity table in phase 15).
- Sensitive: skip on `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`, `CanUploadToCloudClipboard=0`; process denylist via `GetClipboardOwner`→PID→exe name.
- Exit with lazy entry on clipboard: render small reps (`OleFlushClipboard` w/ eager small files ≤ 20 MiB) else clear our entry.

## Architecture
```
apps/windows/src/Fsync.Interop/
  ClipboardListener.cs, SensitiveFormatFilter.cs, ClipboardFormats.cs, CfHtmlCodec.cs
  VirtualFileDataObject.cs, RemoteFileStream.cs (IStream → core.read_file_range), AsyncCapability.cs
apps/windows/src/Fsync.App/
  Clipboard/{ClipboardService.cs, RepresentationMapper.cs, ClipboardWriter.cs, FileClipStrategy.cs, HotkeyManager.cs}
  Files/{SendToHandler.cs, DropTarget.cs, TransferListViewModel.cs}
  Views/History/{HistoryWindow.xaml(.cs)}, ViewModels/HistoryViewModel.cs
  Views/Settings/{General,Privacy,Storage}Page.xaml(.cs)
```
Data flow out: WM_CLIPBOARDUPDATE → filter → mapper → `publish_clip`. In: `on_remote_clip` → (auto) ClipboardWriter: inline reps immediate, large reps delayed, files via strategy → Explorer IStream.Read → `read_file_range` → file/1.

## Related Code Files
- Create: above. Modify: `Bridge/EngineBridge.cs`, `TrayHost.cs`, settings shell (phase 13 files, same owner).

## Implementation Steps
1. Listener + filter + mapper (CF_HTML codec unit tests).
2. Writer with origin format + delayed rendering for large reps.
3. Virtual files per ADR 010 incl. async capability + progress; eager fallback path + inbox cleanup.
4. Manual mode + global hotkey (`RegisterHotKey`).
5. Send To shortcut + drag-drop to flyout.
6. History window + settings.
7. Exit/flush behavior; ARM64 verification.
8. xUnit tests for mapper/filter/CF_HTML; manual QA checklist.

## Todo List
- [ ] Listener/filter/mapper
- [ ] Writer + delayed rendering
- [ ] Virtual-file paste / eager fallback
- [ ] Manual mode + hotkey
- [ ] Send To + drag-drop
- [ ] History + settings
- [ ] Exit flush behavior
- [ ] Tests

## Success Criteria
- Text/html/image round-trip win↔mac; Explorer paste of 1 GiB remote file with progress + cancel, hash verified; 0 password-manager items synced; idle CPU ~0%.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Clipboard locked by other app (OpenClipboard fails) | H x L | retry w/ backoff (5x 50 ms) |
| COM threading bugs (STA) | M x H | dedicated STA thread for clipboard ops; stress tests |
| Explorer hang on slow relay fetch | M x H | async capability + prefetch; eager for small |

## Rollback
- Strategy enum switch to eager via setting; user can switch to manual mode.

## Security Considerations
- Never store filtered items; inbox `%LOCALAPPDATA%` user ACL; Send To IPC only accepts same-user instance; validate paths.

## Next Steps
- Phase 15, 16.
