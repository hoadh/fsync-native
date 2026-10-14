# Code Review: Windows phase 14 (clipboard, files, history, settings)

Branch: origin/windows/phase-14 (7 commits d083b26..6b87203, base 85f8968). 75 files, ~4.7k lines.
Method: read-only review of a git archive of the branch (no dotnet on macOS). Line numbers are from branch files. Not read in full: XAML, SettingsPage code-behind, tests ClipboardMappingTests, FilesTests, RealEngineViewModelTests, ImageCodecTests, ClipboardReadTests.

## Score: 6.5 / 10

Interop layer (OLE data object, lazy IStream, listener, STA host, UI marshalling) is careful and tested against the real clipboard and shell. Four gaps block merge: privacy default list not applied, received files never cleaned, received clips lack history/cloud opt-out flags, clipboard write/publish not serialized.

## Compatibility with current main

- logout() signature unchanged (no args, ffi engine.rs:77); FFI contract still v2; core/fsync-ffi has no diff. ExpectedContract = 2 and EngineBridge.LogoutAsync (EngineBridge.cs:115) fine.
- "No session callback after logout returns" rule: EngineDelegateAdapter.OnSessionChanged -> secrets.SaveSession (Rust thread, SecretStore locked) then ClearSession is safe.
- start() is idempotent on the same engine, so the 15 s GoOnlineAsync retry (App.xaml.cs:97-106, EngineBridge.cs:84-104) is OK.
- Read-ahead in read_file_range: no app change needed (RemoteFileStream caps chunks at 1 MiB, sequential).
- engine.lock is at DataDir\core\engine.lock. App single-instance key is SHA256 of lowercased DataDir string (AppSettings.cs); path spelling variants (trailing slash, 8.3, junction) bypass redirection. Second instance's Start() throws Invalid("another fsync engine...") -> Offline, retried every 15 s forever with the raw message (UserMessages `Invalid i => i.msg`). MEDIUM.
- ResetHistoryAsync -> InitializeAsync (EngineBridge.cs:124-130) overwrites _engine without Stop/Dispose. Safe today only because the ctor failed with HistoryKeyMismatch. Dispose old engine first.
- Quit() (App.xaml.cs:134-144): order Clipboard before Bridge is right, but StaThread.Dispose joins only 5 s, so STA may still be in fetch_rep/read_file_range when the engine is disposed. In-flight calls hold the engine Arc so engine.lock stays held (up to 30 s read deadline); a quick restart gets Invalid.
- Tests use RealEngine in SignedOut state, never Start(); engine.lock not exercised. E2E uses distinct data dirs (W1, W2, devA).
- E2E race (LOW): clipboard-with-cli.ps1: $fetch CLI (devA dir) may still run when `Connect-AppWithCli $c $w2 ... 'devA'` does `pair join` as devA -> engine.lock Invalid. Add `$fetch.P.WaitForExit()` first.

## Must fix before merge

### H1 Privacy: default denylist not applied (HIGH)
ClipboardService.cs:115 uses `s?.denylist ?? SensitiveFormatFilter.DefaultDenylist`. Core default denylist is empty (core/fsync-core/src/settings.rs), so after settings load the password-manager list (SensitiveFormatFilter.cs:56) is unused unless the user clicks Reset+Save (ClipSettingsViewModel.cs:94). ADR 013 s7 claims a default list; macOS always applies builtIn union user.
Fix: union built-in with user list at read time.

### H2 Inbox never cleaned (HIGH)
Eager downloads go to inbox\<itemId> (ClipboardService.Incoming.cs:105-119); nothing deletes them though the plan marks "inbox cleanup" done. Received, possibly sensitive files persist; copy-back creates "name (1)" duplicates. macOS trims by retention days and bytes.
Fix: sweep at startup and periodically by retention/size, skipping files referenced by the current clipboard object.

### H3 Received clips lack history/cloud opt-out (HIGH)
ClipDataObject.cs:96-124 (EnumFormatEtc) offers only entries plus FsyncOrigin; no ExcludeClipboardContentFromMonitorProcessing, CanIncludeInClipboardHistory=0, CanUploadToCloudClipboard=0. Peer content (E2E-encrypted in transit) can reach Win+V history and the Microsoft cloud clipboard.
Fix: add the flags, keep FsyncOrigin as echo marker.

### H4 Unserialized write/publish; unobserved exceptions (HIGH)
- OnRemoteClip does `_ = WriteAsync(item)` (Incoming.cs:36), no newest-wins; a slow eager download/convert finishing late overwrites a newer clip. Replaced ClipDataObject never Abort()ed (`_current = obj`, Incoming.cs:86).
- ReadAndPublishIfAuto -> `_ = PublishAsync(snap)` (ClipboardService.cs:126) can reorder A/B; macOS has a one-at-a-time queue with privacy recheck.
- PublishAsync catches only FsyncException/IOException/UnauthorizedAccess/InvalidOperation (ClipboardService.cs:146-155); InvalidDataException, COMException (WIC), OOM, ArgumentException become unobserved.
Fix: single-reader channel or generation counter per direction plus a logging catch-all.

### H5 Eager strategy: hang, no cap, folder structure (HIGH)
- Task.WhenAll(waits) has no timeout/cancel (Incoming.cs:104-118); offline peer hangs WriteAsync forever and leaks _downloads TCS. macOS bounds to 180 s.
- No eager size cap/hybrid: macOS lazy over 20 MiB, eager cap 512 MiB. Plan said "eager small files <= 20 MiB"; code does Eager for all sizes.
- CF_HDROP built from per-file paths, so copied folders likely paste flat. Verify.
- _finished (Incoming.cs:124) collects every finished Receive transfer not owned by DownloadAsync (e.g. Files-page pulls), never trimmed.

### H6 Quit hang; InitializeAsync no dispose (HIGH/MEDIUM)
- Clipboard.Dispose runs `_sta.Invoke(ReleaseClipboard)` on the UI thread without timeout; OleFlushClipboard of lazy reps up to 20 MiB does blocking network fetches -> exit freeze. No try/finally, so other exceptions skip _sta.Dispose().
- Dispose/Stop old engine in InitializeAsync before re-creating.
- Show a clear "already running" message for the engine.lock Invalid and stop retrying.

## Medium (follow-ups)

6. StaThread.cs: wake-up via PostThreadMessage with null hwnd; nested modal loops in OLE calls can drop it (lost wakeup). Use a message-only window or MsgWaitForMultipleObjects+event. Dispose posts WM_QUIT and drops queued work, so Invoke/InvokeAsync waiters hang; Dispose from the STA itself self-joins; Invoke has no timeout.
7. All work serialized on one STA (renders, each lazy IStream.Read up to 30 s, settle reads, hotkeys). Deliberate per ADR 013 s1, but a stalled peer blocks clipboard processing and UI-thread _sta.Invoke. ClipEntry.Render holds _lock during network fetch.
8. ClipboardReader.cs:49,55: Win32Clipboard.Size() calls GetClipboardData for every wanted format (forces other apps' delayed render) before the maxBytes check. Owner process name read before OpenClipboard (ClipboardReader.cs:35), racy; null owner bypasses denylist. Max size compared to HGLOBAL size, not wire size.
9. HistoryViewModel.cs:81: OnQueryChanged => `_ = RefreshAsync()` per keystroke, no debounce/cancel; interleaved refreshes can duplicate rows or corrupt the cursor.
10. TransfersViewModel.cs:71-83: AcceptAsync removes the offer before pulls; mid-way failure loses the rest with no retry. Auto-accept has no size cap (macOS 1 GiB). OnPullFailed retry uses `message.StartsWith("network")`. `_ = RetryLaterAsync` catches only FsyncException.
11. ClipboardService.cs:87-88: default hotkeys Ctrl+Alt+C/V equal AltGr on many layouts (Polish AltGr+C) and steal input. Default off or change chord.
12. VirtualFile.Normalize (VirtualFile.cs:38-47): good traversal protection (rejects "..", ".", absolute, invalid chars incl ':', trailing '.'/' ', >259) but not reserved device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9, with extensions). Duplicates and invalid entries are silently dropped. Path.Combine(_inboxDir, item.itemId) (Incoming.cs:107) is safe (core Uuid) but add macOS-style hex/dash check. Eager path relies on core fetch_file_to sanitization (place.rs: no overwrite, confined to dest dir).
13. ClipDataObject.cs:27,72: _streams grows forever. Abort() only sets a flag; Explorer cancel just stops reading (no core cancel API; acceptable). PendingLazyBytes counts PNG and CF_DIB at PNG size (Incoming.cs:145).
14. ImageCodec: no pixel-size bound (decompression bomb from paired peer while STA blocked); CF_DIB loses alpha (BitmapAlphaMode.Ignore); `colors * 4` uint overflow in DibToBmpFile; BI_ALPHABITFIELDS ignored.
15. ADR 013 s7 says password managers' "documented private formats are skipped regardless", but SensitiveFormatFilter checks only ExcludeClipboardContentFromMonitorProcessing, Clipboard Viewer Ignore, CanIncludeInClipboardHistory==0, CanUploadToCloudClipboard==0, FsyncOrigin. Fix doc or code. Owner denylist matches exe name only.

## Low

- ClipboardListener: sequence-number arg unused; exceptions go to Trace not AppLog.
- AppSettings.cs doc comment garbled ("Downloadssync").
- ClipSettingsViewModel: 50 MiB default rounds to 52 MB and Save rewrites it (lossy).
- SendTo.cs:36 SendToArgs.Parse accepts UNC and \\?\ paths (File.Exists on UNC may trigger SMB auth); IPC is same-user.
- DevicesPage.OnDrop: empty path list silently ignored.
- TrayHost/hotkey handlers use `_ = Task`; unhandled-exception handler logs e.Message/stack (may include paths).
- Out of scope: SecretStore.Load throws on Unprotect failure at startup, unhandled in async void OnLaunched.

## Parity vs macOS

| Area | macOS | Windows branch |
|---|---|---|
| Denylist | built-in union user | user list only after settings load (H1) |
| Inbox retention | days + bytes | none (H2) |
| File strategy | lazy >20 MiB, eager cap 512 MiB, hybrid | Lazy or Eager, no cap (H5) |
| Eager wait | bounded 180 s | unbounded (H5) |
| Publish | queue, newest wins, privacy recheck | unserialized (H4) |
| Auto-accept cap | 1 GiB | none (M10) |
| History/cloud opt-out on received clip | n/a | missing (H3) |

## Test gaps

Real clipboard, real shell paste (PasteHelper) and real offline engine are used; no mocks. Named semaphore `Local\fsync-clipboard-tests` serializes them. Missing: eager download path; inbox cleanup; ClipboardService echo suppression end to end; exit policy via ReleaseClipboard; StaThread dispose with pending work; Abort/IStream lifetime; denylist via real process owner; engine.lock scenarios; reserved device names in Normalize. FailingRemoteRead assertion is weak (passes if file absent). CodecTests.cs:70 covers "../evil.txt", `C:\abs.txt`, "con?.txt".

## Positives

- UI marshalling correct (EngineDelegateAdapter posts all but OnSessionChanged via ui.Post; SaveSession locked).
- Skip checks run before content reads; flags use names and DWORDs only.
- Sound COM details: CCW ComVisible, FormatEnumerator, SetData bounded 64 KiB, GetData validates lindex via QueryGetData, RemoteFileStream maps errors to STG_E_READFAULT / E_ABORT, FILEDESCRIPTORW layout and ShellLink vtable order correct.
- ClipEntry.Render retries after failure; AppPreferences atomic save; Dialogs guard fixes ContentDialog crash.
- All files under 200 lines (max 183, TransfersViewModel.cs). Plan status honest (done with concerns; ARM64 run and manual QA open).

## Unresolved questions

1. Does CF_HDROP from per-file eager paths flatten copied folders? Needs manual Windows test.
2. How do the Win+V/cloud flags interact with the FsyncOrigin marker?
3. plans/reports/windows-track-261015-0900-phase-14.md was not in the archive I extracted; confirm it is on the branch.
4. Plan/ADR 010 say eager for small files <= 20 MiB, code does Eager for all sizes. Intended?
