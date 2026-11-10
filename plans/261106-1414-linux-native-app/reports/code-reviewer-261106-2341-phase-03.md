# Code review: phase 03 clipboard backends

Score 8.5/10. Critical 0, High 0, Medium 5, Low 12. platform lib tests 85 pass.

## Medium
- M1 X11 echo WriteGuard never cleared (echo.rs:51-58, sync/shared_inbound.rs:104-106, x11_watch.rs:120-121): own write change dropped in watcher, guard stays armed; later legit copy of same content silently skipped as OwnEcho. Fix: clear guard on any non-matching outbound read (like core EchoGuard) or drop app guard.
- M2 X11 SelectionClear race (x11_watch.rs:126, x11_owner.rs:104-107): stale clear after a newer set() -> owned=false, table cleared while X server still lists us -> every paste refused. Fix: compare clear time vs set time / generation; re-check get_selection_owner.
- M3 GNOME QuickClipWindow may act unfocused (QuickClipWindow.qml:93, 51-59; quick_clip_run.rs:255-264): fallback button runs without win.active (stale read published); paste clears pending without confirming ownership; needsClick window never closes. Fix: requestActivate then act on activeChanged; confirm ownsClipboard/marker before clearing pending; ~15 s hide timeout.
- M4 wayland_watch.rs:119 std::env::var per selection change on dispatch thread vs GUI qputenv (app_identity.cpp:32) -> glibc getenv/setenv UB. Fix: read HYPRLAND_INSTANCE_SIGNATURE/SWAYSOCK/XDG_RUNTIME_DIR once at start.
- M5 uri-list guard weaker than assumed (formats.rs:24-27, path_denylist.rs:26-35): any native/Flatpak client can offer FM type; denylist misses ~/.aws, ~/.kube, ~/.docker/config.json, ~/.netrc, ~/.config/gh, browser profiles, /proc, /sys, /dev. Fix: extend denylist, allow only under $HOME, /media, /mnt, /run/media; document in ADR 014.

## Low
1 files >200: Main.qml 204, boot.rs 207, tests/common/mod.rs 233. 2 compositor IPC budget per call not total; connect no timeout; runs in dispatch. 3 GNOME path reads full clipboard before cap (clip_access.rs:28-37). 4 X11 text-only clipboard managers cause one downgraded re-send. 5 SyncEngine::shutdown worst case ~1.35 s > STOP_BUDGET 1.2 s comment. 6 no app-level 30 s inbound budget (core-bounded; document or add). 7 ClipboardStatus pause switch binding broken after toggle. 8 IPC/tray paste blocks thread up to 2 s. 9 hub.quick_clip answers ok when ClipboardObject not registered. 10 X11 TIMESTAMP/MULTIPLE refused, req.time unchecked, reply type unchecked. 11 Wayland pending offer map leak. 12 STRING/TEXT on GNOME path labelled Latin-1 under Qt X11.

## Unresolved
- Dolphin copy: FM MIME offered?
- Mutter honouring token for requestActivate right after show() (LIVE).
- Manual mode: restrict clip-send IPC callers?
