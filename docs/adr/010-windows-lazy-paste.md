# ADR 010 — Windows clipboard: listener, sensitive filter, virtual-file lazy paste, tray
Status: accepted for the automated part (Windows 11 x64, 2026-10-14). The per-app manual matrix,
real password managers, Windows 10 22H2 and ARM64 hardware are still open (see end).

Spike: `spikes/windows-clipboard` (`ClipSpike.Core` library, `ClipSpike.SelfTest` console,
`ClipSpike.Tray` WinUI 3 app). Evidence: `spikes/windows-clipboard/results/`.

## Decision
1. **Lazy paste: delayed-render `IDataObject` with `FileGroupDescriptorW` + `FileContents`
   (one `IStream` per `lindex`), plus `IDataObjectAsyncCapability`.** Eager fallback (download to
   `%LOCALAPPDATA%\fsync\inbox`, `CF_HDROP`) is kept only for targets that want `CF_HDROP` (phase 14
   `FileClipStrategy`), not as the default.
2. **Listener:** message-only window (`HWND_MESSAGE`) + `AddClipboardFormatListener`, created on
   the WinUI 3 UI thread (it pumps Win32 messages). No helper thread needed; `StaThread` stays as
   the fallback and for headless use.
3. **Skip rules** (any one skips the change; only names and the DWORD flag formats are read):
   `FsyncOrigin` (our own write), `ExcludeClipboardContentFromMonitorProcessing`,
   `Clipboard Viewer Ignore`, `CanIncludeInClipboardHistory` = 0, `CanUploadToCloudClipboard` = 0.
4. **Exit policy: clear, never flush.** On exit, if our virtual-file entry is still current,
   `OleSetClipboard(NULL)`. `OleFlushClipboard` keeps `FileGroupDescriptorW` but drops
   `FileContents`, which leaves a broken entry. Small text/image clips we published are normal
   rendered formats and are unaffected.
5. **Tray:** H.NotifyIcon.WinUI 2.1.4 in an unpackaged, self-contained WinUI 3 app
   (Windows App SDK 1.6.250205002). WPF tray fallback not needed.
6. **Core requirement (owner A): sequential read-ahead in `read_file_range`.** The shell reads one
   256 KiB chunk at a time, never two in flight, so lazy paste throughput = 256 KiB / round trip.

## Results (automated, `ClipSpike.SelfTest`, x64)
| Check | Result |
|---|---|
| Listener: in-proc writer, other process (`clip.exe`), 50 rapid writes | every update seen, 1-100 ms; 50/50 |
| Filter: each skip rule above, plus `CanIncludeInClipboardHistory=1` passes | all correct |
| Own `VirtualFileDataObject` seen by the listener | `OwnEcho` (FsyncOrigin) |
| Cross-process `OleGetClipboard` read (3 files incl. 0 bytes, `IStream` proxied) | hashes match |
| Shell "Paste" verb into a folder (Explorer's copy engine), 3 files / 5 MiB | 0.77 s, hashes match; the shell calls `StartOperation`/`EndOperation(S_OK)` (async extraction), sets `Performed DropEffect` = COPY |
| Same, 200 MiB, fake remote 2 ms per read | 13 s, 800 reads x 256 KiB, hash matches, "Copying..." progress dialog (`FD_PROGRESSUI`). Bound by the fake data generator, not by COM |
| Same, 20 MiB, 50 ms per read (relay-like RTT) | 5.5 s (~4 MiB/s): 80 serial reads x 50 ms dominate. Max 1 read in flight in every run |
| Remote read fails after ~20 MiB of 200 MiB | the shell shows "Error Copying File or Folder" in 1.9 s, deletes the partial file, no hang; `EndOperation` waits for the dialog |
| Owner exits without flush / is killed | others see only `DataObject`, `Ole Private Data`; reads fail with `DV_E_FORMATETC` (graceful) |
| Owner exits after `OleFlushClipboard` | descriptor stays, `FileContents` gone: broken (hence decision 4) |
| Owner clears on exit | clipboard empty |

WinUI 3 smoke (`ClipSpike.Tray --smoke`): tray icon created, 21/21 updates delivered on the UI
thread, filter and echo work in-process, clear-on-exit works. ARM64 builds of both apps compile
(not executed: no ARM64 host).

Interop notes for phase 14:
- Implement `System.Runtime.InteropServices.ComTypes.IDataObject` and declare `OleSetClipboard`
  etc. by hand; CsWin32's own `IDataObject` is a different type. CsWin32 for the Win32 calls.
- HGLOBALs from another process are moveable: always `GlobalLock`, never use the handle as a pointer.
- `FileContents` is listed once in `EnumFormatEtc` with `lindex = -1`; `GetData` gets the real index.
- The shell also asks for `Shell IDList Array`, `CF_HDROP`, `EnterpriseDataProtectionId` (we
  return `DV_E_FORMATETC`) and sets `DropDescription`, `TargetCLSID`, `Logical Performed DropEffect`
  (accept and store them).
- The owner must be an STA thread that keeps pumping (the WinUI UI thread is fine). `IStream::Read`
  is called on the shell's copy thread, so the range reader must be thread-safe and blocking.
- Read-ahead sizing (decision 6): with RTT r and target throughput T, keep about T x r bytes in
  flight (e.g. 50 MiB/s x 100 ms = 5 MiB, i.e. ~20 chunks of 256 KiB) once two consecutive reads
  are sequential.

## Not verified / open
- Real Ctrl+V in an Explorer window and on the Desktop. The automated check uses the shell's
  "Paste" verb in a child process (same copy engine, other process). Use the tray menu "Offer 3 test
  files" / "Offer 200 MiB test file", then paste.
- App matrix: Outlook / Teams attachment, Chrome upload (should), VS Code, Slack (nice).
- Real password managers (1Password, KeePass, Edge password copy): only their documented formats
  were simulated.
- Windows 10 22H2, and x64 + ARM64 hardware runs of the tray.
- Cancel from the Explorer progress dialog (only a failing remote read was tested).
