# Phase 03 — Clipboard backends (Wayland data-control, thin X11, GNOME manual window), filters, format map, echo, lifecycle

## Context Links
- [plan.md](plan.md), [phase-02](phase-02-engine-bridge-and-app-shell.md), [clipboard research](../reports/researcher-261106-1354-linux-clipboard-hotkeys-secrets.md) §1-4, §8, [red team](reports/red-team-261106-1438-plan-review.md) #7 #8 #9 #10 #11 #12 #14 #15
- Parity: `docs/adr/013-windows-clipboard-app.md` (decisions 2-5, 7, 8, 11, 12, 14), `apps/windows/src/Fsync.Interop/Clipboard/SensitiveFormatFilter.cs` (built-in password-manager list), `apps/macos/FsyncMac/Sources/FsyncMacCore/Clipboard/SensitiveFilter.swift`
- Core: `core/fsync-core/src/service/clips.rs` (`publish_clip`, kind from first rep, `fetch_rep`), `types.rs` (`LocalClip`, `ClipItem`, `Published.echo`), `core/fsync-core/src/settings.rs` (`clip_mode`, `max_clip_bytes`, `denylist`); core inline budget 256 KiB (reps up to it arrive in the offer)
- References for x11rb XFixes: yclip, clip-bridge sources (research §2)

## Overview
- Priority: P1. Status: In Progress (implemented 2026-11-06; LIVE checks open). Estimate: 6 d. Requires plan Gate **GNOME VM provisioned**.
- Auto sync on Hyprland/Sway/KDE/COSMIC (data-control) and X11 sessions; manual mode everywhere; GNOME Wayland = manual via a focused mini window.

## Key Insights
- data-control gives no source identity: Wayland sensitive filter = MIME hints (`x-kde-passwordManagerHint`, `org.nspasteboard.ConcealedType`) + best-effort focused app from compositor IPC. <!-- red-team 2026-11-06 #9 --> X11: owner window -> XRes `XResQueryClientIds` (client PID, works without `_NET_WM_PID`) + `WM_CLASS`; `/proc/<pid>/comm` when the PID is in our namespace.
- Skip before read (ADR 013 d4): classify on MIME list first; read bytes only for allowed MIMEs, with size cap and timeout. Never read hinted or own entries.
- <!-- red-team 2026-11-06 #10 --> Inbound is always eager: fetch every rep (`fetch_rep` for reps > 256 KiB) before writing the entry; entry holds only ready bytes. No lazy reps, no prefetch, no serve-time fetch timeout. Cost: big images are downloaded even if never pasted (bounded by `max_clip_bytes`).
- Our entries carry marker MIME `application/x-fsync-origin` (own echo, no read needed) and `x-kde-passwordManagerHint: secret` (keeps received content out of clipboard managers, like ADR 013 d12). Core `Published.echo` is the second guard.
- Wayland writing keeps serving only while we own the selection; `cancelled` = lost ownership, normal.
- <!-- red-team 2026-11-06 #7 --> GNOME Wayland: no data-control; only a focused surface may read/write. Design: hotkey/launcher -> `fsync-linux --clip-send|--clip-paste` forwards `XDG_ACTIVATION_TOKEN` over IPC -> running app activates a small `QuickClipWindow` with that token (Qt `QWindow::requestActivate` + `XDG_ACTIVATION_TOKEN` env) -> window focused -> `QClipboard` read (send) or write (paste) -> window closes. Never read the clipboard from an unfocused surface (would return stale/empty data and look like success). No XWayland auto watcher (dropped).
- <!-- red-team 2026-11-06 #8 --> `text/uri-list` can be set by any client (Flatpak app, web page via browser) -> auto-publishing it is a confused deputy. Auto file clips only when the entry also offers a file-manager type (`x-special/gnome-copied-files` or `application/x-kde-cutselection`); otherwise ignore the uri-list (text reps still sync). Path denylist always applies.
- <!-- red-team 2026-11-06 #14 --> X11 thin: receive INCR (needed to read big copies), but never send INCR: entries larger than the server's max request size are skipped for X11 write with a status message; cap ~ max request (typically 16 MiB with BIG-REQUESTS, measured at runtime).
- <!-- red-team 2026-11-06 #11 --> Lifecycle policy (ADR 013 d5/d14 counterpart, v1): the entry we own disappears when the app quits (Wayland/X11 serve from our process); documented in ADR 014 + README, not worked around. Shutdown gate stops serving new requests before `stop()`.

## Requirements
Functional:
- `detect.rs`: override `FSYNC_CLIPBOARD_BACKEND=wayland|x11|manual`; else `WAYLAND_DISPLAY` -> bind ext-data-control-v1 -> zwlr-data-control-v1 -> else Manual (GNOME); no Wayland + `DISPLAY` -> X11; none -> Manual. Result + reason shown in status.
- Outbound (auto): change -> filters -> reps in order `text/html` (only with text/plain), `text/plain` (from `text/plain;charset=utf-8` | `UTF8_STRING` | `text/plain`), `image/png`; `text/uri-list` -> `LocalClip.files` only with a file-manager type present, local `file://` paths, absolute, existing, not denied. Size > `max_clip_bytes` -> skip + status. PNG header dims > 16384/side or > 64 MP -> skip.
- Path denylist (outbound files, also 04a send): anything under `$data_dir`, `~/.ssh`, `~/.gnupg`, `~/.local/share/keyrings`, `~/.password-store`, `~/.config/fsync`; checked after canonicalize. Folders otherwise allowed. <!-- red-team 2026-11-06 #8 -->
- Inbound (auto): `on_remote_clip` -> eager fetch of all reps (own thread, 30 s budget, newest wins) -> write entry with MIME aliases (`text/plain;charset=utf-8`, `UTF8_STRING`, `STRING`, `TEXT`, `text/plain`; `text/html`; `image/png`) + marker + hint. Files kind -> 04a handler. <!-- red-team 2026-11-06 #10 -->
- Manual (`ClipMode::Manual` or GNOME): nothing published until `clip-send` (IPC/tray/window button); received clip held as pending until `clip-paste`; tray shows pending. On Manual backend, `clip-send`/`clip-paste` always go through `QuickClipWindow`. <!-- red-team 2026-11-06 #7 -->
- Sensitive filter <!-- red-team 2026-11-06 #9 -->: hints, marker; X11 source = XRes PID -> `comm`/exe basename, plus `WM_CLASS`; Wayland source best-effort: Hyprland `hyprctl -j activewindow` (class, pid) via socket, Sway `swaymsg -t get_tree` focused node (app_id, pid); other compositors -> none. Built-in names (ported Windows/macOS lists + `keepassxc`, `bitwarden`, `1password`, `enpass`, `kwalletmanager5/6`, `seahorse`, `org.keepassxc.KeePassXC`, `com.bitwarden.desktop`) + user `denylist`; case-insensitive. Unknown source -> allowed (accepted limit). First-run notice (once, dismissable): "On Wayland, password managers without a privacy hint may be synced; use Manual mode or the denylist" + current filter capability.
- Lanes (ADR 013 d11): one outbound + one inbound lane, newest wins, every task observed and logged (no content in logs). Publishing runs on the outbound lane thread, not on phase 02 op threads. <!-- red-team 2026-11-06 #12 -->
- Lifecycle: register with phase 02 shutdown gate: stop watchers, refuse new reads/writes, finish in-flight serve <= 1 s, drop sources; then core `stop()`. <!-- red-team 2026-11-06 #11 -->
- Pause sync toggle in tray (app-local, not a core setting).
Non-functional: change-to-publish < 300 ms for text; no busy polling; watcher threads stop within 1 s on quit; files < 200 lines.

## Architecture
```
platform::clipboard (no Qt)
  Backend trait: kind(); events() -> Receiver<ClipChange>; read(change, mime, cap, timeout) -> Vec<u8>;
                 write(OwnedEntry) -> Result; owned() -> bool; shutdown()
  ClipChange { mimes: Vec<String>, source: Option<SourceApp{pid, name, class}>, token }
  OwnedEntry { reps: Vec<(mime aliases, Vec<u8>)>, marker, hint }        (bytes only, eager)
  wayland_*: watcher thread + source thread serving fds (crate vs raw per spike A)
  x11_*: x11rb conn thread: XFixes OWNER_CHANGE, hidden owner window, SelectionRequest/Clear, INCR receive only, XRes PID
  source_app.rs: X11 XRes/WM_CLASS; Wayland compositor IPC (hyprland, sway)
platform::sync (no Qt, depends on fsync-core types)
  outbound: ClipChange -> filters -> formats::to_local_clip -> publish_clip (outbound lane thread)
  inbound:  ClipItem -> fetch all reps (inbound lane thread) -> formats::to_entry -> backend.write
  lanes: newest-wins slots, cancel tokens, shutdown()
app::bridge::clipboard_object (QObject): backendName, mode, pending, paused, filterCapability; sendNow(), pasteNow(), togglePause()
app::bridge::quick_clip (QObject + QuickClipWindow.qml): activate(token, action) -> QClipboard read/write via cxx-qt-lib (or tiny C++ shim) -> close
```
Data flow out (auto): compositor/X server -> watcher thread -> `ClipChange` -> filters (MIME, marker, source app, uri-list FM-type + path denylist) -> read allowed reps (cap, 2 s timeout) -> `LocalClip` -> publish (outbound lane) -> status queued to Qt.
Data flow out (GNOME): `fsync-linux --clip-send` -> IPC {cmd, activation_token} -> QuickClipWindow focused -> QClipboard mime data -> same filters (no source app) -> publish.
Data flow in: core callback thread -> `ClipItem` -> inbound lane -> fetch reps -> backend.write (auto) or pending (manual) -> paste via backend or QuickClipWindow.

## Related Code Files
Create (under `/home/alex/Work/fsync-native/apps/linux/`):
- `platform/src/clipboard/{mod.rs,detect.rs,formats.rs,filters.rs,path_denylist.rs,echo.rs,source_app.rs,wayland_watch.rs,wayland_source.rs,x11_conn.rs,x11_watch.rs,x11_owner.rs,denylist_builtin.rs}`
- `platform/src/sync/{mod.rs,outbound.rs,inbound.rs,lanes.rs}`
- `app/src/bridge/{clipboard_object.rs,quick_clip.rs}`, `app/qml/{ClipboardStatus.qml,QuickClipWindow.qml,FirstRunNotice.qml}`
- `tests/e2e/clipboard-with-cli.sh`, `tests/e2e/lib-headless-sessions.sh` (nested headless sway + Xvfb helpers, prereq check)
Modify: `app/src/main.rs`, `app/src/shutdown.rs` (clipboard step), `app/src/bridge/{mod.rs,engine_object.rs,delegate_adapter.rs}`, `app/src/tray.rs` (pause, pending, send/paste), `app/qml/Main.qml`, `platform/Cargo.toml` (wl-clipboard-rs or wayland-client + wayland-protocols(staging, wlr), x11rb (xfixes, res), png header), `platform/src/lib.rs`, `/home/alex/Work/fsync-native/docs/adr/014-linux-desktop-app.md` (spike, GNOME design, lifecycle policy, filter limits)

## Implementation Steps
1. Spike A (<= 1 day, throwaway branch): wl-clipboard-rs `Watcher` + `copy` on Hyprland: can we serve from our thread with our own MIME list (marker) and get `cancelled`? Does the watcher expose MIMEs before bytes? Pass -> crate; else raw `wayland-client`. Record in ADR 014. (XWayland-auto spike dropped.) <!-- red-team 2026-11-06 #7 -->
2. `formats.rs` + `filters.rs` + `path_denylist.rs` + `echo.rs` with unit tests (MIME alias tables, uri-list parse (CRLF, percent-decoding, reject non-file/remote hosts), uri-list without FM type ignored, denied paths incl. symlink into `~/.ssh`, hint detection, PNG dimension bounds, denylist match on comm/class/app_id).
3. X11 backend (x11rb): conn thread, XFixes OWNER_CHANGE on CLIPBOARD, TARGETS, conversion with property cleanup, INCR receive, owner window answering SelectionRequest (TARGETS + MIME list, no INCR send: oversize -> skip + status), SelectionClear -> lost. <!-- red-team 2026-11-06 #14 -->
4. `source_app.rs`: XRes client PID + `WM_CLASS`; Hyprland/Sway IPC queries with 200 ms timeout; unit-tested parsers on captured JSON. <!-- red-team 2026-11-06 #9 -->
5. Wayland backend per spike A: bind manager (ext preferred, zwlr fallback), device for default seat, watch selection, read via pipe with timeout, source with marker + aliases, serve from bytes, ignore primary selection.
6. `sync/` lanes + outbound/inbound (eager fetch on inbound lane, never on callback thread); `shutdown()` hooks into phase 02 gate. <!-- red-team 2026-11-06 #10 #11 -->
7. `clipboard_object.rs` + tray actions + `ClipboardStatus.qml` (backend name, auto/manual, pending, paused, filter capability, last error) + `FirstRunNotice.qml`.
8. GNOME manual path: `quick_clip.rs` + `QuickClipWindow.qml` (frameless small window, "Sending..."/"Pasted" label, closes after 1 s); IPC `clip-*` with activation token; tray actions use the tray's own activation. Test in GNOME VM. <!-- red-team 2026-11-06 #7 -->
9. E2E `clipboard-with-cli.sh` <!-- red-team 2026-11-06 #15 -->: prereq check first (`sway`, `wl-copy`/`wl-paste`, `Xvfb`, `xclip`, `dbus-run-session`; missing -> exit 2 with `pacman -S` hint, not a FAIL); each app instance in `dbus-run-session` with `FSYNC_SECRETS=file`. Nested headless sway (`WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway -c minimal.conf`, own `WAYLAND_DISPLAY`) so the user's clipboard is untouched. Non-text inbound via second app instance L2 in a second headless sway. Cases: text out, html out (kind `rich`), png out (kind `image`, size), hinted copy not sent, uri-list without FM type not sent, uri-list into `~/.ssh` not sent, text in (CLI `clip put` -> wl-paste), html + png L1 -> L2 (sha256 equal), marker present, echo not re-sent (CLI sees exactly one clip), manual pending until `--clip-paste`, quit while owning entry exits < 6 s. Repeat X11 subset under `Xvfb` + `xclip` with `FSYNC_CLIPBOARD_BACKEND=x11`.

## Todo List
- [x] Spike A recorded in ADR 014
- [x] formats / filters / path denylist / echo + unit tests
- [x] X11 thin backend (INCR receive only, XRes PID, WM_CLASS)
- [x] source_app (X11, Hyprland, Sway)
- [x] Wayland backend (ext + zwlr)
- [x] sync lanes, outbound, inbound (eager), shutdown hook
- [x] ClipboardObject + tray + status QML + first-run notice
- [x] GNOME QuickClipWindow with activation token
- [x] e2e on headless sway + Xvfb PASS (clipboard-with-cli.sh twice; Wayland, manual QuickClipWindow offscreen, X11)

## Success Criteria
- `cargo test --manifest-path apps/linux/Cargo.toml -p fsync-linux-platform clipboard sync` green.
- `apps/linux/tests/e2e/clipboard-with-cli.sh` PASS twice (Wayland + X11 subsets); text latency printed <= 1 s locally.
- Manual on Omarchy Hyprland: copy in browser -> CLI receives within 2 s; CLI `clip put` -> paste in another app; KeePassXC copy not sent (hint or `activewindow` class).
- GNOME VM: hotkey-bound `fsync-linux --clip-send` pops the mini window and publishes; `--clip-paste` makes text pasteable in gedit/Text Editor.
- `build-in-container.sh ubuntu-2204` smoke still exit 0.

## Pending Live Verification
<!-- red-team 2026-11-06 #14 --> KDE, Sway live session, X11 desktop (Xfce/i3) -> ADR 014 "untested" (Sway/X11 covered headless by e2e).
- [x] LIVE(blocking): GNOME VM manual send/paste via QuickClipWindow + activation token — evidence: ADR 014 table (2026-11-07)
- [ ] LIVE: Hyprland KeePassXC / Bitwarden copy not sent — deferred 2026-11-12 (no manager on the test host; roadmap)

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| wl-clipboard-rs cannot serve from a thread | M x M | Spike A first; raw wayland-client fallback (budget +2 days) |
| ext-data-control staging churn | L x M | zwlr fallback; pin wayland-protocols |
| GNOME refuses focus without valid token (hotkey path) | M x M | Fallback: window shows "Click to send/paste" button (user click = focus); documented |
| Eager download of big images wastes bandwidth | M x L | `max_clip_bytes` cap; newest wins cancels |
| Clipboard loop between two fsync devices | M x H | Marker MIME + core echo + lane dedupe; e2e asserts single send |
| Hints and source absent on non-Hyprland/Sway Wayland | H x M | First-run notice, Manual mode, documented limit |
| Confused deputy via uri-list | M x H | FM-type requirement + path denylist; e2e cases |

## Security Considerations
- No clipboard content or file names in logs; status shows kinds/sizes only.
- Hinted/denied content never read, published or stored. Received entries carry the hint.
- Caps on read size and image dims before decoding; uri-list paths local, absolute, existing, FM-typed, not under denied roots (after canonicalize).
- Never read from an unfocused surface on GNOME.

## Rollback
- Backends behind `detect.rs`: `FSYNC_CLIPBOARD_BACKEND=manual` disables auto sync without a rebuild; revert phase commits otherwise (phase 02 shell unaffected).

## Next Steps
- Phase 04a plugs file clips (inbox + uri-list write) and history copy-back into `inbound`/backend write.
