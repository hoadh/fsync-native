# Code review: phase 04b (settings, send to a device, hotkey help)

Score: **7.5 / 10**. Critical: 0. High: 2. Medium: 5. Low: 8.

## Scope
- Rust: `app/src/engine/{send_files,settings_ops,shortcut_help,ipc_handler,e2e_control,startup}.rs`, `app/src/bridge/{settings_object,devices_model}.rs`, `platform/src/clipboard/{path_denylist,uri_list}.rs`, `platform/src/ipc/{protocol,server}.rs`; core `service/{clips,walk}.rs` (contract check)
- QML: `DevicePicker`, `SendDropArea`, `SettingsView`, `SettingsClipboardForm`, `ShortcutHelp`, `Main` (picker/drop wiring), `DevicesView`
- Checks: `cargo test --workspace` green (app 45, platform 120, integration all ok). Clippy clean on both feature sets.

## Overall
Security core holds: re-validation in the running instance, canonicalize + allowlist + "folder contains a root" check, core `send_files` re-checks trust (`NotPaired`) and `walk::expand` skips symlinks (top level and nested), so a removed device or a symlink swapped in after validation cannot leak. Thread rules are respected (fs/core work on op threads, results via `CxxQtThread::queue`). The e2e guard is intact. The real gaps: the path that is validated is not always the path that is sent (non-UTF-8 names), the picker hides *what* is staged, `--send` on a cold start is silently dropped, and pending state is not cleared when the session changes.

## Critical
None.

## High

**H1. Validated path ≠ sent path for non-UTF-8 names** — `engine/send_files.rs:114-117`
`validate` returns canonical `PathBuf`s (raw bytes from `parse_uri_list`, which supports non-UTF-8 names), then `send_to` converts with `to_string_lossy()` because core takes `Vec<String>`. Scenarios:
- Drop `~/Docs/\xff.pdf`: it passes validation, then core gets `~/Docs/\u{FFFD}.pdf`. If no such file exists you get an Io error ("Not sent."), and that only shows up after the user picked a device.
- Worse: if `~/\xff` (a validated dir) and `~/\u{FFFD}` both exist, core walks a directory that was **never validated** (it could contain or be a hardlink or bind-mount target; symlinks are skipped, so the impact is limited to same-uid-planted content). The invariant "only validated paths reach core" is broken.
Fix: in `validate`, refuse paths whose `to_str()` is `None` (new code, e.g. `path-unsupported`, text "File names that are not UTF-8 cannot be sent yet.") and carry `String` from then on. The IPC client already lossy-encodes (`protocol.rs:144`), so `--send` of such a file fails earlier with a misleading "path does not exist". Document the limit in ADR 014.

**H2. The DevicePicker does not show what will be sent; concurrent stages silently replace each other** — `send_files.rs:73`, `DevicePicker.qml:15-46`
`stage` overwrites `pending_send` (last writer wins) from `--send`, from a window drop and from another `--send`. The picker shows only devices, never the file count or names. Failure scenarios:
- (a) User runs `--send a.pdf`, then drops `b/` on the window before clicking. One click sends `b/` only, and `a.pdf` is dropped without notice.
- (b) Any same-uid process (a script, an "Open with" helper) stages `~/Documents/taxes/` right after the user's own `--send`. The user's click then confirms a set they never saw.
Same-uid is inside the IPC trust boundary, but the picker is the user's consent step, so it should name the set. Fix: expose `pendingSummary` ("3 files: a.pdf, b.txt, ...", or "1 folder: Photos"; GUI only, never logged) on the picker, and have `pick` take a stage generation id so a click answers the set that was shown (if the generation differs, refuse with "The files changed, pick again").

## Medium

**M1. `--send` on a cold start is silently ignored** — `engine/startup.rs:20-30` (only `forward` uses `args.send`)
With no running instance, `fsync-linux --send f` becomes the instance, boots and never stages `f`. The exit code is 0 and no picker opens. README:75-78 and `--help` say it "opens the device picker". Desktop "Open with fsync" / Nautilus scripts hit this whenever the app is not autostarted. Fix: after boot reaches Ready, stage `args.send` once (same `stage` path), or print "fsync is starting; run the command again" and exit 1. Document whichever you choose (`--clip-send` has the same cold-start behaviour, which ADR "never queued" covers, but `--send` has no such note).

**M2. Staged set and picker survive sign-out and account change** — `core_state/mod.rs:68`, `engine/ops_logout.rs`, `Main.qml:193`
`pending_send` is cleared only by `cancel_pending` and `pick`, and the modal `devicePicker` is not closed when `page` leaves 2. Scenario: picker open → session expires or the user signs out from the tray → sign-in page with the modal picker still on top. After a sign-in (possibly another account), a click sends the old set to a device of the new account. Fix: `send_files::cancel_pending` in the logout and reset-account paths (next to `reset_account_files`), and `onPageChanged: if (page !== 2) devicePicker.close()`. Use `close()` that does not emit `rejected` twice, or make `cancelPick` idempotent (it already is).

**M3. IPC `Send` does fs work on the single IPC server thread** — `ipc_handler.rs:35` → `send_files::stage` → `canonicalize` ×≤1000; `server.rs:93` serves inline
The header says "every branch answers fast". `stage` canonicalizes up to 1000 paths (plus `exists()` in `protocol.rs:84`, which predates this phase) on a path list that may sit on a hung `/mnt` NFS/SMB mount (explicitly allowlisted). The IPC thread blocks with no timeout. All hotkeys (`--clip-send`/`--clip-paste`) stop answering, launchers report "cannot reach the running instance", and `IpcServer::stop` joins that thread, so **quit can hang**. Fix: answer `ok` after a cheap syntactic check and stage on an op thread (refusal → banner text like drops), or run the stage on an op thread with a bounded wait (e.g. 1.5 s < IO_TIMEOUT) and reply `busy` on timeout.

**M4. Settings save can silently rewrite values the SpinBoxes cannot hold** — `SettingsClipboardForm.qml:58,63`, `settings_object.rs:90-91`
`retention` `to: 2000000` MiB and `maxClip` `to: 100000` MiB, plus `clamp_i32`. A stored `retention_bytes` above about 1.9 TiB, or `max_clip_bytes` above about 97 GiB (set by the CLI or another platform's settings), displays clamped. `apply` then sees `f != cur.div_ceil(MIB)` and writes the clamped value when the user saves an unrelated field such as the denylist. That contradicts the ADR 014 promise "a byte value set elsewhere is never altered by a save" and the comment at `settings_ops.rs:46-47`. Fix: send a per-field "touched" flag from QML (or keep the loaded MiB in the object and compare against *that*, not against the recomputed `cur`), and raise `to:` to `2147483647`.

**M5. QML URL stringification + Rust `trim` change file names** — `SendDropArea.qml:62`, `send_files.rs:55`, `uri_list.rs:36`
`drop.urls[i].toString()` is `QUrl::toString()` (PrettyDecoded): spaces come out decoded. A name with leading or trailing spaces (`"report "`) is then trimmed by both `urls_to_paths` and `parse_uri_list`. You get MISSING, or a *different* existing file (`report`) is validated and sent. Fix: pass the URLs encoded. In QML, `encodeURI` is wrong; better to make the invokable take `QList<QUrl>`/`QVariant` and call `toEncoded()` in Rust, or use `Qt.resolvedUrl`-free `drop.text` when it is a uri-list. Alternatively, trim only `\r` in `urls_to_paths`. Add a test with a trailing-space name.

## Low

- **L1** `send_files.rs:83-88`: `pick` takes the set before core checks trust. If the device was removed meanwhile, core returns `NotPaired`, the error shows, and the whole staged set is lost (the user must re-drop). Acceptable; to improve, put the set back on `NotPaired`/`PeerOffline`.
- **L2** Hardlinks are not detected by canonicalize: a hardlink in `~/Docs` to `~/.ssh/id_x` passes. Only same-uid can plant one. Add it to the documented limits in `path_denylist.rs:1-8` and ADR 014 next to Flatpak `~/.var/app`.
- **L3** Unbounded TOCTOU window between stage and pick (the picker can stay open for hours). Mitigated by core `walk` skipping symlinks at every level. Intermediate path components are still followed: swapping `~/Docs` for a symlink to a dir that contains the same file name leaks that file. Same-uid only. Cheap hardening: re-run `validate` inside `pick`/`send_to` on the op thread before `svc.send_files`.
- **L4** `DevicePicker.qml:35-38` lists every account device incl. ones not paired with us (needs-pairing). The click gives a `NotPaired` error. Filter to trusted rows if `DeviceRow` exposes it.
- **L5** `Dialog` `closePolicy` default includes CloseOnPressOutside; whether that emits `rejected` on Qt 6.2 is unverified. If it does not, `cancelPick` is skipped and the set lingers (harmless today, but it combines with M2). Set `closePolicy: Popup.CloseOnEscape` or handle `onClosed` instead of `onRejected` (and guard against a double call after accept).
- **L6** `MAX_ITEMS` (`send_files.rs:20`) duplicates `ipc::protocol::MAX_PATHS` (1000). Reuse the constant (DRY). An empty/oversized set returns `nothing-staged`; for >1000 a "too many files" text would be clearer.
- **L7** `parse_uri_list` `percent_decode` accepts `%+1` (`u8::from_str_radix` allows a sign). Harmless after validation; strict hex check (`is_ascii_hexdigit` ×2) is better hygiene.
- **L8** File sizes: `send_files.rs` 223 and `devices_model.rs` 207 lines are over the 200 target (under the 500 hard limit). Moving the `send_files` tests to `send_files_tests.rs` (as in `offers_tests.rs`) fixes the first. E2e `clipboard-with-cli.sh:365` only logs when "Files sent" is missing; it does not assert it (Send-event wiring has no automated assertion).

## Verified OK (focus list)
- Relative paths / `..`: the client absolutizes (`protocol.rs:143`) and the server requires absolute paths. `check` refuses non-absolute paths and canonicalizes, so `..` is resolved before the allowlist. Test `file:///a/../b` stays lexical until validate, which is fine.
- `file://` host part: empty host or `localhost` only, any other host refused. Foreign schemes refuse the whole drop. NUL after decoding is refused. Tests cover this.
- Symlink into a denylisted dir is refused (test). A folder containing a root (e.g. `~`) is refused. Core `walk` skips nested symlinks.
- Removed device between staging and pick: core `is_trusted` → `NotPaired`. No send happens.
- Threads: `dropped`/`stage` fs work on op threads (except M3). Settings load/save run on op threads with `qt_thread().queue`. `pick_device` on the GUI thread only does locks plus a spawn.
- `retention_soon` is called on a days/size change. Concurrent sweeps are safe (per-item `begin_download` claim). Clip-mode change → `refresh_mode` → `note_mode` (status only, no pending-clip side effects); Auto↔Manual is safe.
- Shortcut help: single source, no Ctrl+Alt, test enforces it. `TextField.copy()` works on Qt 6.2 (no QML Clipboard type needed). Note: in Auto mode the copied bind line is itself synced as a clip (expected self-copy).
- e2e `pick-device` is behind `cfg(feature = "e2e-control")`. The release `compile_error!` guard in `main.rs:5-13` is intact.
- No file names logged: `send_files`, `settings_ops`, `devices_model` have no `tracing` calls with paths. IPC "path does not exist: {p}" goes back to the same-uid caller only, not to logs.
- DRY: the drop parser reuses `uri_list::parse_uri_list` (now `pub`). Settings `clip-mode` e2e reuses `settings_ops::save`.

## Recommended actions (priority)
1. H1: refuse non-UTF-8 in `validate`; carry `String` to core.
2. H2: show the staged summary in the picker; generation id on pick.
3. M1: cold-start `--send` (stage after Ready, or exit 1 with a message).
4. M2: clear `pending_send` and close the picker on sign-out/page change.
5. M3: move IPC `Send` staging off the IPC thread.
6. M4/M5: settings clamp and drop URL encoding.
7. Low items as convenient; L3 re-validate-at-send is cheap.

## Metrics
- Tests: workspace green (app 45, platform 120, integration suites ok).
- Clippy `--all-targets`: 0 warnings / 0 errors on both default and `FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control`.
- Files over 200 lines (new/modified this phase): 2 (L8). None over 500.

## Unresolved questions
- Does Qt 6.2 `Dialog` emit `rejected` on CloseOnPressOutside? (L5)
- Should `--send` on a cold start queue (contradicting "never queued") or refuse? This is a product decision (M1).
- Does `DeviceRow` carry a trusted/paired flag the picker can filter on? (L4)
