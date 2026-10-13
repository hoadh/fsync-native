# Phase 05 — Spike: Windows clipboard listener + virtual-file lazy paste + tray

## Context Links
- Design §4 flow 5, §5 risks (lazy paste High, WinUI 3 tray Low-Med)
- Gates: [phase-14](phase-14-windows-clipboard-files-history.md)

## Overview
- Priority: P0 | Status: pending | Effort: 1.5-2 wk (D; overlaps phase 03 days)
- Validate: `AddClipboardFormatListener` from WinUI 3 app (hidden message window), privacy formats filter, delayed-render `IDataObject` with `CFSTR_FILEDESCRIPTORW` + `CFSTR_FILECONTENTS` (IStream) pasted into Explorer & apps, H.NotifyIcon tray in WinUI 3.

## Key Insights
- Clipboard ownership: our `IDataObject` via `OleSetClipboard`; must keep STA thread alive and call `OleFlushClipboard` only if we want to drop laziness (on exit). If app exits, lazy entry dies → on exit flush small items or clear.
- Explorer reads `FILECONTENTS` via `IStream::Read` on its thread, possibly blocking UI → Rust `read_file_range` must be blocking-safe with timeout; Explorer shows progress dialog for large files if `FD_FILESIZE` + `FD_PROGRESSUI` set.
- Sensitive formats: skip if `ExcludeClipboardContentFromMonitorProcessing` present, or `CanIncludeInClipboardHistory`/`CanUploadToCloudClipboard` = 0.
- Self-write marker: register custom format `FsyncOrigin` → skip echo.

## Requirements
Test matrix (lazy remote file 200 MiB + multi-file 3 items):
| Target | Lazy expected |
|---|---|
| Explorer Ctrl+V | must |
| Desktop | must |
| Outlook / Teams attachment, Chrome upload | should |
| VS Code, Slack | nice |
Also tray icon + menu + click works on Win10 22H2 & Win11, x64 + ARM64.

## Architecture
```
spikes/windows-clipboard/
  ClipboardListener.cs     # HWND_MESSAGE window, WM_CLIPBOARDUPDATE
  SensitiveFormatFilter.cs # RegisterClipboardFormat checks
  VirtualFileDataObject.cs # IDataObject: GetData(FILEDESCRIPTORW|FILECONTENTS lindex) → IStream
  RemoteFileStream.cs      # IStream: Read → simulated range fetch (later core.read_file_range)
  TrayHost.cs              # H.NotifyIcon.WinUI
```
COM interop via CsWin32 source generator (no hand-written P/Invoke where avoidable).

## Go / No-Go
| Check | Go | Fallback |
|---|---|---|
| Explorer lazy paste incl. progress, cancel | works, no Explorer hang > OS default | **Eager fallback**: download to `%LOCALAPPDATA%\fsync\inbox\<item>` (cap 512 MiB) then `CF_HDROP` with real paths |
| Apps (Outlook/Chrome) | lazy works | hybrid: also offer `CF_HDROP`? (only possible after download) → eager for ≤ 20 MiB |
| Listener in WinUI 3 process | events fire reliably, no leaks | dedicated Win32 helper thread |
| H.NotifyIcon in WinUI 3 | stable tray + menu | WPF host for tray (design fallback) |

## Related Code Files
- Create: `spikes/windows-clipboard/**`, `docs/adr/010-windows-lazy-paste.md`

## Implementation Steps
1. Tray host + listener + filter; log formats from 1Password, KeePass, Edge password copy.
2. Implement `VirtualFileDataObject` + `RemoteFileStream` with simulated latency; Explorer test.
3. App test matrix; ARM64 run.
4. Test app exit while lazy entry on clipboard (expected: paste fails gracefully; define behavior).
5. ADR 010.

## Todo List
- [ ] Tray (H.NotifyIcon) + listener
- [ ] Sensitive format filter
- [ ] Virtual file IDataObject + IStream
- [ ] App test matrix + ARM64
- [ ] Exit behavior defined
- [ ] ADR 010

## Success Criteria
- ADR 010 with per-app results and chosen strategy; filter skips all password-manager copies tested.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| COM IDataObject quirks (lindex, TYMED, async) | H x M | CsWin32; reference known open-source virtual-file implementations; timebox 1 wk then eager |
| Explorer UI freeze on slow network | M x H | `IDataObjectAsyncCapability` (async extract) so Explorer copies in background |

## Rollback
- Spike isolated; strategy behind `FileClipStrategy` in phase 14.

## Security Considerations
- No content logging; only format names + sizes.

## Next Steps
- Phase 14 implements chosen strategy; phase 03 consumes C# findings.
