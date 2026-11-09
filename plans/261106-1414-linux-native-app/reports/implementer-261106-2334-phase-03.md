# Phase 03 implementation report (clipboard backends)

Status: DONE_WITH_CONCERNS. Phase file stays "In Progress" (open LIVE items: GNOME VM, Hyprland password managers).

## Built (per todo item)
- Spike A recorded in ADR 014 (see below). Verdict: raw wayland-client.
- formats / filters / path denylist / echo + tests: `platform/src/clipboard/{formats,uri_list,filters,path_denylist,echo,denylist_builtin}.rs`.
- X11 thin backend: `x11_{conn,read,watch,owner,backend}.rs` (XFixes, INCR receive via own short-lived connection + selection timestamp, XRes PID, WM_CLASS, no INCR send, STRING only when Latin-1).
- source_app: `source_app.rs`, `source_app_ipc.rs` (Hyprland `j/activewindow`, Sway `get_tree`, 200 ms, parsers unit-tested on JSON).
- Wayland backend: `wayland_{proto,dispatch,watch,loop,reader,source,backend}.rs` (ext preferred, zwlr fallback, one thread, bounded pipe I/O, serve on short-lived writer threads, stop flag).
- sync: `platform/src/sync/{lanes,outbound,inbound,engine,shared,shared_inbound,status}.rs` (newest-wins lanes, eager inbound, pending/paste, pause, shutdown(budget)).
- App: `engine/{clipboard_runtime,clip_service_adapter,clipboard_actions,quick_clip_run}.rs`, `bridge/{clipboard_object,quick_clip,clip_access}.rs`, `cpp/clip_access.{h,cpp}`, QML `ClipboardStatus` (in Settings tab), `FirstRunNotice`, `QuickClipWindow`; tray: Send now / Paste received clip / Pause sync (checkmark); IPC `clip-send`/`clip-paste` wired (token forwarded); shutdown step "stop clipboard" before core stop (budget arithmetic updated in `engine/shutdown.rs`); boot starts the engine after the service exists; `on_remote_clip` queues to the inbound lane; smoke covers the new QML.
- e2e: `tests/e2e/{clipboard-with-cli.sh,lib-headless-sessions.sh,lib-app-instance.sh}`, `platform/examples/clip_offer.rs` (multi-MIME owner), integration tests `platform/tests/clipboard_{wayland,x11}.rs` (private headless sway / Xvfb, skip when missing).

## Spike A result (nested headless sway, wl-clipboard-rs 0.9.5)
Watcher gives MIME list before bytes, serving own MIME list (marker) works, `cancelled` ends `serve()` (1.04 s). FAIL: `serve()` writes inline with a blocking fd inside dispatch; a paster that never reads a 4 MiB entry kept the serve thread blocked 12 s after another client took the selection; no stop handle. So "refuse new, finish in-flight <= 1 s, drop source" (plan lifecycle gate) is impossible -> raw wayland-client 0.31 + wayland-protocols(staging ext) + wlr. Integration tests cover a stalled paste during replace and shutdown (< 1.5 s). zwlr fallback tested by forcing the protocol (`start_with`).

## Deviations
- `OwnedEntry` = `Vec<EntryRep{mime, offered aliases, data}>`; marker and hint are ordinary reps appended by `to_entry` (backends stay generic).
- `SyncSettings.active` (state Ready): signed out the clipboard is not read and received clips are dropped (not in phase text; privacy).
- Platform crate now depends on fsync-core + fsync-proto (types); new deps: wayland-client 0.31.15, wayland-protocols 0.32.13 (client, staging), wayland-protocols-wlr 0.3.12, x11rb 0.14 (xfixes, res), rustix feature "pipe". `check-lock-drift.sh`: none (301 crates match; extra block-padding only).
- Phase-02 files touched: `pair-with-cli.sh` (app must not see the user's real session: unset DISPLAY/WAYLAND/HYPRLAND/SWAYSOCK, `QT_QPA_PLATFORMTHEME=generic`; `clip-paste` now expects ok because the Manual backend opens the quick window), `ipc_handler.rs` (+tests), `delegate_adapter.rs`, `boot.rs`, `shutdown.rs`, `tray.rs`, `core_state/{mod,hub}.rs`, `Main.qml`, `SettingsView.qml`, `build.rs`, `platform/src/ipc/protocol_e2e.rs` (+`clip-mode` e2e command, also in the no-e2e negative test), `e2e_control.rs` (+`clip`, `online_peers` in status, `clip-mode`). Phase 02 text pointed at `app/src/shutdown.rs`/`tray.rs`; real paths used.
- Wayland first-run notice shows only on the Wayland backend; marker file `<data_dir>/clipboard-notice-seen`.
- Inbound Files kind: reported in status only; `SyncEnv.on_files` hook for 04a.
- Tray labels refresh when ksni re-reads the menu (not verified live; no watcher in the headless env).

## Verification (apps/linux, QMAKE=/usr/bin/qmake6)
- `cargo fmt --all --check` ok; `cargo clippy --all-targets -- -D warnings` ok; same with `--features fsync-linux/e2e-control` ok.
- `cargo test --workspace` ok (platform lib 85, wayland 3, x11 2, ipc 9+5+2, app 28; 0 failed).
- `scripts/check-lock-drift.sh` ok.
- `--smoke` and `FSYNC_SMOKE_CORE=1 --smoke` exit 0.
- `tests/e2e/pair-with-cli.sh` PASS.
- `tests/e2e/clipboard-with-cli.sh` PASS twice (Wayland, manual-backend window path offscreen, X11). Text latency copy -> CLI: 33-66 ms Wayland, 20-38 ms X11. Quit while owning an entry: L2 0.1 s, L1 2.5 s (core stop with peers), X11 0.1 s (all < 6 s).
- `scripts/build-in-container.sh ubuntu-2204` exit 0 (Qt 6.2 qmlcachegen of the new QML + smoke).
- All sessions private (headless sway, Xvfb, `FSYNC_SECRETS=file`, isolated runtime/data dirs); processes, docker, dirs cleaned. One slip: during the spike I ran `wl-paste -l` once against the real session (MIME names only, no content).

## Remaining for live checks
- LIVE(blocking): GNOME 46 VM: `--clip-send` pops the window and publishes; `--clip-paste` pastes in Text Editor; real xdg-activation token path (headless runs have no token; offscreen QPA activates on show).
- LIVE: Hyprland KeePassXC/Bitwarden copy not sent (hint or `activewindow`); tray menu behaviour with a real StatusNotifier host; KDE, live Sway, Xfce/i3 stay "untested" in ADR 014.

## Concerns
- Pairing finding (phase 02 area): a device in `needs-pairing` must OFFER; the CLI refuses `pair join` there with a clear message, but the app's `pair-join` from that state shows "Pairing failed: network: connection lost" (confusing). Worth a clearer message in a later phase.
- Wayland sensitive filter limits as documented (no hint + not focused/listed -> synced); XRes PID of a process that forks after connecting is the exited parent (xclip default) -> unknown source.
- GUI thread blocks in `QClipboard` reads on the GNOME path (bounded by Qt's own timeout).
- Publish is not retried when no peer is online ("Not sent: no other device is reachable", as ADR 013).
- Files over 200 lines: `platform/tests/common/mod.rs` (233, test helpers) and shell scripts (exempt).

## Unresolved questions
- Should Manual mode on a working backend show a visible "held clip" counter in the tray label only, or also a notification (notify-rust) when a clip arrives pending?
- Is dropping `STRING` for non-Latin-1 text on X11 acceptable, or should it be offered as lossy Latin-1?
