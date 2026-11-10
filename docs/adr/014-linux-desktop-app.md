# ADR 014 — Linux desktop app: Qt6/QML over CXX-Qt, distro floor, packaging shape
Status: accepted (2026-11-10; drafted in phase 01 of `plans/261106-1414-linux-native-app`). Acceptance condition: the phase 06 Windows GUI legs X3 and X4 pass. They pass in `docs/test-results/linux-cross-os-261110-1241.json`. Open items in "Open" do not block acceptance. Cross-OS results: see "Cross-OS results (phase 06)" below.

## Context
fsync has macOS (SwiftUI over UniFFI) and Windows (WinUI 3 over uniffi-bindgen-cs) apps.
`fsync-core` and `fsync-cli` already run on Linux. Targets: Omarchy/Arch, Ubuntu, Debian,
Fedora, x86_64. The UI toolkit was chosen by the owner: Qt6/QML.

## Decisions
1. **Rust + CXX-Qt + QML, no FFI layer.** `apps/linux` QObjects are written in Rust with
   CXX-Qt and call `fsync_core::service::FsyncService` directly. uniffi-bindgen-cpp supports
   only uniffi 0.28.3; the repo pins `=0.29.4` (ADR 008), so a C++ binding is not viable.
   CXX-Qt is pinned exactly (`=0.10.0`): its bridge syntax changes between minors.
2. **Own Cargo workspace** (`apps/linux/Cargo.toml`, crates `platform` without Qt and `app`
   with Qt). The root `cargo test --workspace` never needs Qt. The app lock is seeded from the
   root lock; `scripts/check-lock-drift.sh` fails if fsync-core's runtime dependency tree
   differs from the root (291 crates identical at phase 01).
3. **All bridge sources in `app/src/bridge/`.** cxx-qt-build allows one directory per QML
   module (QTBUG-93443); a bridge file elsewhere fails the build.
4. **Distro floor: Ubuntu 22.04 / Debian 12** (glibc 2.35 / 2.36, Qt 6.2.4 / 6.4.2). Release
   builds run in an `ubuntu:22.04` container; the binary needs `GLIBC_2.34` at most.
5. **One binary for .deb, .rpm and AppImage.** The Qt 6.2-built binary passes `--version`,
   `--smoke` and core smoke against distro Qt 6.11.2 (fedora:latest) and Qt 6.12.0
   (archlinux:latest). Newer Qt most likely rejects the 6.2 qmlcachegen units and compiles the
   embedded QML source instead (not measured: slightly slower first load); either way the
   smoke shows the QML loads and runs. The PKGBUILD builds from source against Arch Qt and must be rebuilt on Qt minor
   bumps (standard for Arch Qt packages).
6. **AppImage bundles the build container's Qt 6.2** (same binary as .deb/.rpm). Accepted
   limit: QtWayland 6.2 lacks later fractional-scaling fixes; HiDPI users on rpm/deb/Arch get
   distro Qt. Revisit (pinned Qt 6.8 LTS build) if AppImage bug reports show Qt 6.2 issues.
7. **`--smoke`**: loads the QML root, exits 1 on a load failure or any QML warning (message
   handler in `app/cpp/smoke_log.cpp`, teardown warnings included). Phase 01: it also waits
   for a worker-thread update through `CxxQtThread::queue` and exits 2 after 5 s without it;
   phase 02 replaces this check with the engine bridge. `FSYNC_SMOKE_CORE=1` also opens and closes a
   throwaway `FsyncService` (SQLCipher/OpenSSL vendored). Used by container and package checks.
8. **Platform on the wire, two steps.** This plan makes `Platform` decoding forward-compatible
   (unknown index preserved and shown as Cli); the Linux app sends `Cli`. Emitting `Linux` is a
   later release, gated on the prod backend and the Mac/Windows apps shipping the tolerant
   decoder (see "Platform emit gate").

## Phase 01 spike results (2026-11-06)
| Check | Result |
|---|---|
| Omarchy host, Qt 6.12, debug build, `--smoke` + core smoke | pass |
| `--smoke` with a broken QML binding | exit 1 (warnings counted and printed) |
| `ubuntu:22.04` (Qt 6.2.4) release build + both smokes | pass, 6 min cold build |
| `debian:12` (Qt 6.4.2) release build + both smokes | pass |
| 6.2 binary in `fedora:latest` (Qt 6.11.2) | pass |
| 6.2 binary in `archlinux:latest` (Qt 6.12.0) | pass |
| Release binary | 17 MB; NEEDED: libQt6Gui, libQt6Qml, libQt6Core, libstdc++, libgcc_s, libc, libm |
| Max glibc symbol | GLIBC_2.34 |

Notes: without `lld` or `mold`, cxx-qt-build links with `gold` (Rust warns it is deprecated);
the build images install `lld`. Qt Quick is loaded as a QML plugin at runtime, so packages
must depend on the QML modules, not only on the libraries above (derived in phase 05).

## GNOME test host
`ubuntu-gnome` (ssh alias in the owner's `~/.ssh/config`, Tailscale): Ubuntu 26.04.1 LTS,
GNOME Shell 50.1 (distro Qt 6.10.2), Hyper-V VM on the WASO host. Used for the GNOME manual-mode checks (phase 03).

## Platform emit gate (future)
The Linux app may emit `Platform::Linux` only after the prod backend and Mac/Windows app
releases containing the tolerant decoder are deployed and adopted. Until then the cargo
feature `emit-platform-linux` stays off in every packaged build.

- Current state (2026-11-09): gate closed. The app sends `Platform::Cli` (`app/src/engine/boot.rs`);
  `emit-platform-linux` is off by default (`app/Cargo.toml`).
- The tolerant decoder is in `fsync-proto` (`platform.rs`, commit c2d1e4f): `Linux` decodes, any
  other index is kept as `Unknown(n)` and re-encoded unchanged. The FFI shows `Linux` and unknown
  platforms as `Cli`.
- Not yet done: no deploy of the backend and no Mac/Windows release carrying this decoder is
  recorded in this plan. Opening the gate needs both, plus adoption.
- Cross-OS evidence (phase 06) ran with the app emitting `Cli`; the Windows peer accepted it unchanged.

## Untested desktops
KDE Plasma 6, Sway live sessions, X11 desktops other than Xvfb, Thunar/KWallet integration.
Supported by design (data-control, X11 backend), not verified on hardware in this plan.

## Clipboard (phase 03, 2026-11-06)
9. **Wayland backend: raw `wayland-client` 0.31 + `wayland-protocols` (ext-data-control-v1,
   staging) + `wayland-protocols-wlr`, not `wl-clipboard-rs`.** One protocol-agnostic layer
   (`clipboard/wayland_proto.rs`) binds ext first, zwlr as fallback; one thread owns the
   watcher and our source; pipe reads and paste serving use bounded non-blocking I/O.
10. **X11 backend: `x11rb` (pure Rust), thin.** XFixes owner change on CLIPBOARD; every read
    uses its own short-lived connection (INCR receive, the selection timestamp of the vetted
    change, so an owner that changed since refuses it); source process from XRes
    `QueryClientIds` (works without `_NET_WM_PID`) + WM_CLASS; our entry is served from the
    watcher thread. No INCR on send: an entry over the server's maximum request is refused at
    write time ("skipped" status). `STRING` is offered only for Latin-1 text.
11. **Detection** (`clipboard/detect.rs`): `FSYNC_CLIPBOARD_BACKEND=wayland|x11|manual` wins
    (also the rollback switch); else a Wayland session that binds a data-control manager;
    a Wayland session without one (GNOME) is Manual even when XWayland exists (XWayland auto
    watching was dropped); else X11 when `DISPLAY` answers; else Manual. The reason is shown.

### Spike A: wl-clipboard-rs 0.9.5 vs raw wayland-client (nested headless sway)
| Question | Result |
|---|---|
| `Watcher` shows MIME types before bytes | yes (`ClipboardEvent::Changed { mime_types, offer }`, bytes via `offer.receive`), cancel handle works |
| Serve our own MIME list incl. a marker from our own thread | yes (`prepare_copy_multi(..).serve()` with `omit_additional_text_mime_types`; `sensitive` adds the KDE hint) |
| `cancelled` when another client takes the selection | yes, `serve()` returned after 1.04 s |
| A paster that never reads (4 MiB entry, paste into a pipe nobody drains) | **fails**: the serve loop writes inline with a blocking fd inside the dispatch handler; after another client took the selection the serve thread was still blocked 12 s later |
| Stop / bound serving at shutdown | **no handle**: `serve()` cannot be cancelled, so "refuse new requests, finish in-flight serve <= 1 s, drop the source" (plan lifecycle gate) is not achievable |
Verdict: functionally enough for reading and writing, but the lifecycle requirement fails and
the dependency tree grows (tree_magic_mini, os_pipe). Raw wayland-client was
the planned fallback; the implementation is about 650 lines in 8 files. Integration tests
(`platform/tests/clipboard_wayland.rs`) cover watch, MIME-first, hint, read cap, stale reader,
write aliases + marker, cancelled, a stalled paste during replace and shutdown (< 1.5 s).
Primary selection is ignored (its offers are destroyed unread).

### Sensitive-content filter and its limits
Order (skip before read): own marker `application/x-fsync-origin`; privacy hints
`x-kde-passwordManagerHint` and `org.nspasteboard.ConcealedType`; source app against the
built-in password-manager list (ported Windows/macOS lists + keepassxc, bitwarden, 1password,
enpass, kwalletmanager5/6, seahorse, `org.keepassxc.KeePassXC`, `com.bitwarden.desktop`, ...)
plus the user's `denylist`, case-insensitive on process name, WM_CLASS and Wayland app id.
- X11: source = owner window -> XRes PID -> `/proc/<pid>/comm`, plus WM_CLASS. Limits: a
  process that forks after connecting (xclip's default) reports the exited parent; a client in
  another PID namespace (Flatpak) resolves to no name; both mean "unknown source".
- Wayland: data-control carries no source identity. Best effort: the focused window from the
  compositor IPC (Hyprland `j/activewindow`, Sway `get_tree`), 200 ms budget; other compositors
  give no source. The focus can differ from the copying app (focus moved between copy and our
  query). Password managers that set no hint and are not focused-and-listed are synced; this is
  an accepted limit (a default-Manual Wayland was rejected). A first-run notice says so and
  names the current filter capability.
- Unknown source -> allowed. Hints are advisory: any app can omit them.
- Received clips carry the KDE hint (clipboard managers skip them) and the marker.

### File clips from the clipboard (confused-deputy guard)
`text/uri-list` can be set by any client, and a file-manager type
(`x-special/gnome-copied-files`, `application/x-kde-cutselection`) can be set by any native or
Flatpak client just as easily, so the type check is a hint, not a security boundary. The path
rules are the boundary. A path must be a local absolute `file://` URI, exist, and after
canonicalize (symlinks and `..` resolved):
- lie under `$HOME`, `/media`, `/mnt` or `/run/media` (everything else, including `/tmp`,
  `/proc`, `/sys`, `/dev` and `/etc`, is refused);
- not lie under (nor be a folder containing, since core expands directories) a protected root:
  the data dir, `~/.ssh`, `~/.gnupg`, `~/.local/share/keyrings` (and `$XDG_DATA_HOME/keyrings`),
  `~/.password-store`, `~/.local/share/kwalletd`, `~/.config/fsync` (and
  `$XDG_CONFIG_HOME/fsync`), `~/.aws`, `~/.kube`,
  `~/.docker`, `~/.netrc`, `~/.config/gh`, `~/.mozilla`, `~/.config/{chromium,google-chrome,
  BraveSoftware}`.
When `HOME` is unset or relative the guard fails closed (no file is ever published). Known
limit: other credential stores (Flatpak app data under `~/.var/app`, other dotfiles, project
`.env` files) are not listed; a clip of ordinary files under `$HOME` is by design what the user
copied. Text reps of the same copy still sync.

### Manual-mode IPC authority
`--clip-send` / `--clip-paste` arrive over the per-user IPC socket whose peer is checked with
SO_PEERCRED (same uid, 0700 runtime dir). Any process of the same user can therefore ask the
running app to publish the current clipboard in Manual mode; that is the same authority such a
process already has over the clipboard itself, so no extra restriction is added.

### Deliberate limits
- X11 echo edge case: without a clipboard manager, the first copy after a received clip is
  swallowed as our own echo when it equals the received content (the write guard is spent by
  the first read after a write).
- GNOME paste is confirmed only after a ~180 ms Qt event-loop round trip (the local read-back
  right after `setMimeData` always succeeds on Wayland): the marker must still be on the
  clipboard, otherwise the clip stays pending.
- No debounce: every copy is published, even copies 25-40 ms apart (newest wins in the lane).
- X11 thin owner: no INCR send, no `MULTIPLE`, no `TIMESTAMP` target (apps that need them fall
  back or fail the paste); an oversize entry gives up the selection instead of serving the old
  one.
- The GNOME path reads a full `QMimeData` format through Qt before the size cap is applied
  (Qt has no sized read); the cap is enforced before anything is published.
- A publish already running inside core cannot be cancelled: quit waits for it up to the
  clipboard budget (1.5 s; the step's real worst case is ~1.9 s, see `engine/shutdown.rs`), then
  abandons it.

### Lifecycle policy (v1, counterpart of Windows decisions 5 and 14)
The entry we own is served from our process (Wayland source / X11 owner window), so it
disappears when the app quits; there is no clipboard manager hand-off. This is documented, not
worked around. Quit order: the shutdown gate closes, IPC stops, then the clipboard stops
(backend first: in-flight pastes see the stop flag within 100 ms and are cut, the source is
destroyed, threads join within ~1 s; then both lanes), and only then core `stop()`. Inbound is
always eager: every rep is fetched (`fetch_rep`, inline reps return at once) before the entry
is written; nothing lazy is ever served. Newest inbound clip wins and cancels the older fetch.

### GNOME Wayland (Manual backend)
GNOME has no data-control, and only a focused surface may read or write the clipboard. Design:
a hotkey or launcher runs `fsync-linux --clip-send|--clip-paste`; the second launch forwards
`XDG_ACTIVATION_TOKEN` over IPC; the running app opens `QuickClipWindow` (small frameless
window) with that token (`QWindow::requestActivate`, token env set for exactly that call, Qt
>= 6.3); only when the window is active does it read the clipboard through `QClipboard` (send)
or write it (paste) and then close after 1 s. If the compositor refuses the focus, the window
shows "Click to send/paste": the click is the user input that gives it. The clipboard is never
read from an unfocused surface (it would return stale or empty data that looks like success).
Filters apply as everywhere, without a source app. The same window serves the tray actions.
Status: implemented; the window path (paste, then send reading the clipboard back through
Qt, and the activation-token plumbing without a token) runs headless in the e2e on the offscreen
Qt platform, which activates windows on show; verified live on GNOME 50 (table below).

| Check | Result |
|---|---|
| GNOME VM (ubuntu-gnome, GNOME 50): `--clip-send` pops the window, publishes | pass 2026-11-07: custom shortcut, focus granted without a click, "Sending" then auto-close ~1 s; text from Text Editor received by a CLI peer (3 presses, 3 deliveries) |
| GNOME VM: `--clip-paste` makes text pasteable in Text Editor | pass 2026-11-07: CLI peer text pasted with Ctrl+V after the window auto-closed |
| Hyprland: KeePassXC / Bitwarden copy not sent | pending (phase 06) |
| KDE Plasma, Sway live session, X11 desktops (Xfce/i3) | untested (Sway and X11 covered headless by e2e) |

## Files, inbox and history (phase 04a)
Parity with ADR 013 d6/d10/d13/d14, minus virtual files (Linux has none, ADR 009 threshold kept).

- **Eager inbox.** A received file clip of at most 20 MiB is downloaded completely into
  `<data_dir>/inbox/<item_id>/` (dirs 0700, `item_id` must match `^[0-9a-f-]{1,64}$`), then written as
  `text/uri-list` plus `x-special/gnome-copied-files` (`copy\n` + URIs, no trailing newline), our
  marker and the password-manager hint. Only top-level entries are listed, so a folder pastes as a
  folder. Core keeps the relative layout (`fetch_file_to` places `dest/<rel_path>`, never overwrites).
- **Name safety.** Every `rel_path` component is checked again before any fetch (no empty, `.`, `..`,
  `/`, NUL, CR/LF, > 255 bytes, and not the reserved `.complete`); the top-level name is taken from
  the path core returns and must stay under the canonical item dir. A link or plain file under an
  item id is never followed (it is replaced by a fresh dir). A failed or cancelled download removes
  its partial dir.
- **Reuse.** `.complete` (top-level names, written last through temp + rename) marks a finished copy;
  a later clip or copy-back of the same item reuses it without fetching. A damaged copy (listed entry
  gone) is fetched again. Budget 180 s per eager item; newer clip cancels an older download through
  the inbound lane's newest-wins cancel.
- **> 20 MiB.** No download and no notification action: a plain notification ("Large file clip (N MiB)
  - open fsync to download") and the item is held (32 newest) for the History Download button, which
  fetches into the inbox (30 min budget) and runs `xdg-open <item dir>`.
- **Retention.** Inbox follows history settings: item dirs older than `retention_days`, then oldest first
  until under `retention_bytes`; the item on the clipboard is kept. Uses `symlink_metadata`, only real
  dirs with a valid id are touched, links are never followed (also not when sizing). Runs at start,
  every 24 h and on a settings change (`inbox_runtime::retention_soon`; the settings screen of 04b
  must call it, until then the e2e `clip-mode` command does).
- **Pushed files** (`on_incoming_files`) show in the Offers tab (sender, count, size): Accept / Decline,
  progress (core ticks every 250 ms, UI pokes limited to 4/s), Cancel, "Open folder". They go to
  `$XDG_DOWNLOAD_DIR/fsync` (`user-dirs.dirs`, else `~/Downloads/fsync`, 0700). Auto-accept only with
  `auto_accept_files` and total <= 1 GiB. Transfers end through `TransferWaiter`, which keeps an early
  final event, so the start/finish race cannot hang a worker.
- **History view.** Search (250 ms debounce), pin, delete, clear (keeps pinned), Copy and Download.
  Copy-back writes the item's reps via `to_entry`, or its inbox files via `files_entry`; with a
  background backend through the engine (marker set, so no re-send), on GNOME through Qt on the
  History window (focused surface). A files item without a finished inbox copy has Copy disabled.
- **Fix round 1 decisions.**
  - *One download per item.* `Inbox` holds a shared in-flight set (one `Inbox` lives on `Core`); a
    second download of the same item (eager, History Download, e2e) is refused with "Already
    downloading." and the History row shows a disabled "Downloading..." button. Two downloads would
    clear each other's directory.
  - *Retention vs downloads (Windows d13 parity).* Items in flight are never removed. A clip the
    user downloaded on purpose gets an `.explicit` marker: it counts toward neither the size
    budget nor its removals, only the age limit (`retention_days`) applies. Marker names
    (`.complete*`, `.explicit*`) are reserved and refused as remote names.
  - *Offers are bounded.* At most 20 active (pending or receiving) offers and 50 rows; further
    pushes are booked as a declined row with a note on the callback thread (no thread, no
    notification). Offer notifications are coalesced to one per 5 s; the tab badge covers the rest.
  - *Sign-out and "this device was removed"* cancel every running download and accepted offer
    (cancel flags plus an epoch counter) and clear offers, held large clips and History rows.
    Shutdown cancels the same way right after the gate closes; a cancelled or failed inbox download
    removes its partial directory.
  - *Core errors are not shown raw:* a failed transfer reads "The transfer failed." (core text can
    contain paths); nothing with a file name is logged.
  - *Models* update rows in place (`dataChanged`) when the ids are unchanged, so a Cancel click is
    not lost to a reset; the Offers role `state` is `offerState` (it shadowed `Item.state`).
  - Deleting History items does not delete their inbox copies (retention does).
  - *Round 2:* every sign-out path (logout, session expired, device removed) goes through
    `inbox_runtime::reset_account_files` (also stales in-flight History loads). Retention takes the
    item's download claim right before removing it, so a download that starts mid-sweep is skipped.
- **Tray** has a History entry (opens the window on that tab).
- **Qt bridge note.** A second `extern "C++Qt"` declaration of `QAbstractListModel` in another bridge
  file makes the cast helpers collide at link time; only `devices_model.rs` declares it, the other
  models alias it (`type QAbstractListModel = crate::bridge::devices_model::qobject::QAbstractListModel`).
- **Test-only IPC (`e2e-control`):** `offer-accept` (newest pending or `input` id), `history-copy`,
  `history-download` (does not open a file manager); status gains `offers` and `history`.

| Check | Result |
|---|---|
| Omarchy: file clip from a second fsync instance pastes in the file manager | pending (manual, not part of 04a automation) |
| GNOME VM: paste of received files in Nautilus | pending (LIVE) |
| Dolphin / Thunar on other desktops | untested |

## Phase 04b decisions (settings, send to a device, hotkey help)
- **Settings screen** (`SettingsView.qml` extended, form in `SettingsClipboardForm.qml`): clip mode,
  retention days / MiB, largest clip (MiB), auto-accept files, app denylist (one entry per line),
  clipboard status with backend, detection reason and filter capability, secrets-in-a-file warning,
  autostart, shortcut help. `SettingsObject` reads and writes through `engine::settings_ops` on an op
  thread (results return through `CxxQtThread::queue`); core validates (`set_settings`), errors are
  shown with the same user text as everywhere. A size is written back only if the user changed it
  (shown rounded up to MiB), so a byte value set elsewhere is never altered by a save.
- **After a save:** a clip-mode change calls `SyncEngine::refresh_mode`; a retention days / size
  change calls `inbox_runtime::retention_soon` (the e2e `clip-mode` command now goes through the same
  save). Help text says days and size also bound the file inbox, and that clips downloaded on purpose
  are age-limited only.
- **Send to a device** (`engine::send_files`): `--send` (IPC) and window drops stage paths after
  re-validation in the running instance with `PathDenylist` (absolute, resolvable after canonicalize,
  only `$HOME`, /media, /mnt, /run/media, no protected root, symlinks resolved). Refusals: IPC error
  `path-protected` / `path-missing` / `nothing-staged` (the launcher exits 1 and prints it); drops show
  a sentence in the banner. A staged set opens the DevicePicker (`section "send"`); a row click calls
  `DevicesModel.pickDevice`, Cancel forgets the set. A drop on a device row sends at once to that
  device; a drop elsewhere on the signed-in window opens the picker. Drops carry `file://` URLs only
  (percent-decoded in Rust by `uri_list::parse_uri_list`; one foreign URL refuses the whole drop).
  Sending runs `FsyncService::send_files` on an op thread; progress text goes to the clipboard status
  line ("Sending to X...", "Files sent." / "Sending failed.") from the core Send events.
- **Hotkey help** has one source, `engine::shortcut_help` (Settings via `SettingsObject.shortcuts`,
  `fsync-linux --help`, README): Hyprland `bind = SUPER ALT, C, exec, fsync-linux --clip-send`, Sway
  `bindsym Mod4+Mod1+c ...`, KDE / GNOME custom-shortcut commands. Never Ctrl+Alt (AltGr).
- **Test-only IPC:** `pick-device` (`input` = device name) answers the picker; e2e covers `--send` ->
  `pick-device` -> CLI `fetch` (sha256) and refused sends (~/.ssh, outside HOME, missing path).

| Check | Result |
|---|---|
| Omarchy: `SUPER ALT, C` bind sends in manual mode; settings survive a restart | pending (LIVE, manual) |
| Drop from Nautilus / Dolphin / Thunar onto a row | untested (only `file://` is accepted) |

### Phase 04b fix round 1
- **Validated = sent.** `send_files::validate` returns canonical UTF-8 `String`s and refuses a
  non-UTF-8 name (`path-unsupported`); core receives exactly these strings, no lossy conversion.
  A path is validated again right before core is called (a symlink or folder swapped in while the
  picker was open is caught). Known limit: hardlinks are not detected by canonicalize (a hardlink in
  `~/Docs` to `~/.ssh/id_x` passes; only the same uid can plant one), like Flatpak `~/.var/app`.
- **The picker names the set it sends.** Staging builds a summary (count, first three names, size,
  folders marked) and a stage id that changes on every stage and clear (`core_state::staged_send`).
  `SendObject` (own QML singleton; moved out of `DevicesModel`) exposes `summary` / `stageId`; a click
  answers the id on display and the engine refuses a different current set (`stage-changed`). A new
  set staged while the picker is open replaces the summary and shows a warning line. An emptied set
  (sign-out, refusal, cancel) closes the picker; only Escape / Cancel dismiss it (a click outside
  would drop the set unseen). A failed send keeps the set and reopens the picker.
- **Sign-out clears it:** `inbox_runtime::reset_account_files` (every sign-out path) clears the staged
  set; the picker also closes when the page leaves the signed-in view.
- **Cold-start `--send`:** when the launch becomes the instance, the files are staged after boot
  reaches Ready and the picker opens (validated in-process like any `--send`). Signed out (or an
  engine error) refuses with a banner sentence ("Sign in first, then send the files again."): files
  are not kept across a sign-in. Waits at most 120 s for Ready. Other commands keep "never queued".
- **IPC thread stays free:** the socket thread no longer touches the file system (the server-side
  `exists()` is gone too). `--send` validates on an op thread; the thread waits for the verdict at most
  2 s, then answers ok ("queued") and a refusal shows in the window. Quit cannot hang on a dead mount.
- **Settings:** a size is sent back only if the user edited the field (`resolve_mib` against the
  loaded value); SpinBoxes go to `i32::MAX`, so stored values are not clamped on display.
- **Drops carry encoded URLs** (`QUrl::toEncoded` per item, as a `QList<QUrl>`); Rust decodes once
  with a strict percent decoder (`%+1` refused). Names are never trimmed.
- **Status:** a slow send outcome no longer overwrites a newer event (`StatusCell::event_seq` /
  `event_if_unchanged`, unit-tested with a slow publish and a concurrent inbound event). This was the
  root cause of the 04a LARGE e2e flake (the stale "Sent to 2 devices." replaced "Large file clip").
- The picker lists every other account device: `Device` has no paired flag; a click on an unpaired one
  shows "That device is not paired." (core re-checks trust).
- Qt 6.2: the QML (picker `closePolicy`, `QList<QUrl>` drop invokables) loads in the ubuntu-2204
  container smoke; the Dialog behaviour itself is not exercised interactively.

- **Round 3 notes.** Every `last_event` writer of the inbound lane counts as an event even when the
  sentence repeats (`StatusCell::update_event`). The launcher checks UTF-8 on the absolute path, so a
  non-UTF-8 cwd plus a relative name gives `path-unsupported`. Limits, accepted: up to 4 staging
  checks stuck on a hung mount make later `--send` / window drops answer `busy` until the app
  restarts (row drops and pick re-checks are not counted, by design: they are user-paced); a stage
  request that is still the latest may finish minutes later and open the picker out of context, the
  summary it shows keeps the click informed.

## Phase 05 decisions (packaging and install smoke, 2026-11-07)
12. **One release binary, three formats.** `scripts/build-release.sh` builds once in the
    `ubuntu:22.04` image (`release.Dockerfile` on top of `build.Dockerfile`), strips a copy, and feeds
    `cargo deb`, `cargo generate-rpm` and linuxdeploy from it. The same `--build-info` assertion runs
    before packaging and again in every install smoke. One `.deb` serves Ubuntu 22.04/24.04 and
    Debian 12 (t64 alternatives); no per-distro build was needed.
13. **Tools pinned.** cargo-deb 3.8.0, cargo-generate-rpm 0.21.1, cargo-about 0.9.3 (`cargo install
    --locked --version`, `--features cli` for cargo-about: without it `cargo install` exits 0 and
    installs nothing, the Dockerfile now tests for the binaries); linuxdeploy `1-alpha-20250213-2`
    and linuxdeploy-plugin-qt `1-alpha-20250213-1` by tag + sha256 (hashes computed once from the
    release assets, GitHub shows no digest for these tags), unpacked into `/opt/linuxdeploy`.
    `.deb` and `.rpm` were byte-identical across rebuilds (`SOURCE_DATE_EPOCH` = commit time).
14. **Release guard layers** (all four exercised, see results): `release-guard.sh` (env + `cargo tree`
    feature graph), `compile_error!` for `e2e-control` in release, `--build-info` empty feature list
    in `package-in-container.sh`, `smoke-checks.sh` and PKGBUILD `package()` (not `check()`, so `--nocheck` cannot skip it).
15. **Desktop file:** `Categories=Network;FileTransfer;` (was `Utility;Network;`):
    `desktop-file-validate` hints against two main categories. `Exec=fsync-linux` stays (no `%F`:
    `--send` is not a plain file argument and an unknown positional would exit 1).
16. **AppImage contents.** Bundled: Qt 6.2 (Gui, Qml, Quick, Controls, Wayland client), QML modules
    found by plugin-qt from `app/qml`, platforms `xcb`, `wayland-egl`, `wayland-generic`, `offscreen`
    (headless smoke), imageformats, tls. plugin-qt does not deploy the Wayland plugin directories
    (`EXTRA_QT_PLUGINS` is deprecated and `EXTRA_QT_MODULES=waylandclient` did not add them), so
    `build-appimage.sh` copies `wayland-shell-integration` (xdg-shell), `wayland-decoration-client`
    and `wayland-graphics-integration-client`, runs `linuxdeploy --deploy-deps-only` on them and fails
    the build if `libxdg-shell.so` is missing. linuxdeploy's excludelist leaves libGL/OpenGL,
    fontconfig, freetype, harfbuzz, libICE/libSM (X11) to the host; the smoke host installs
    `libgl1 libopengl0 libegl1 libfontconfig1 libxkbcommon0 libdbus-1-3 libglib2.0-0 libharfbuzz0b
    libfreetype6 libice6 libsm6` and no Qt package.
17. **PKGBUILD two source modes.** Release: `git+https://...#commit=<40-hex>` (placeholder is
    refused; fill at release time, then `makepkg --printsrcinfo > .SRCINFO`). Local:
    `FSYNC_LOCAL_SRC` tarball of the working tree (used by `smoke-install.sh arch`, nothing is
    committed yet). Rebuild on every Arch Qt 6 minor bump (`pkgrel`). `$srcdir` is remapped out of
    the binary (`--remap-path-prefix`, `-ffile-prefix-map`).
18. **Licenses:** `cargo about` report (`THIRD-PARTY-LICENSES.txt`, ~650 KB) in
    `/usr/share/doc/fsync-linux/` (deb, rpm, AppImage) or `/usr/share/licenses/fsync-linux/` (Arch); the install smoke checks it in all four. fsync's own
    license is `UNLICENSED` in Cargo metadata; the deb `copyright` file says "all rights reserved".

### Runtime dependencies (derived by `scripts/derive-runtime-deps.sh`, not from memory)
Direct NEEDED libraries (readelf): `libQt6Core`, `libQt6Gui`, `libQt6Qml`, libstdc++, libgcc_s,
libc, libm (glibc symbol floor `GLIBC_2.34`). Quick is a run-time QML plugin, so the QML modules
come from `qmlimportscanner -rootPath app/qml` and the Qt platform plugins are listed by hand.

| Source | Ubuntu 22.04 / Debian 12 / 24.04 (`dpkg -S`) | Fedora latest (`rpm -qf`) |
|---|---|---|
| libraries | `libqt6core6 \| libqt6core6t64`, `libqt6gui6 \| libqt6gui6t64`, `libqt6qml6`, `libstdc++6`, `libgcc-s1`, `libc6 (>= 2.34)` | `qt6-qtbase`, `qt6-qtbase-gui`, `qt6-qtdeclarative`, `libstdc++`, `libgcc`, `glibc` |
| QML modules | `qml6-module-qtquick`, `-qtquick-controls`, `-qtquick-layouts`, `-qtquick-templates`, `-qtquick-window`, `-qtqml`, `-qtqml-models`, `-qtqml-workerscript`, `-qtquick-shapes` | all in `qt6-qtdeclarative` (scanner: QtQuick, Controls.Basic(+impl), Layouts, Templates, Shapes, QtQml(+Models, WorkerScript)) |
| platform plugins | `qt6-qpa-plugins` (xcb, offscreen on 22.04), `qt6-wayland` (wayland-egl, wayland-generic) | `qt6-qtbase-gui` (xcb, offscreen, wayland), `qt6-qtwayland` |
| Recommends | `gnome-keyring \| keepassxc \| kwalletmanager` | `gnome-keyring` (weak dep) |
| Arch | `qt6-base qt6-declarative qt6-wayland hicolor-icon-theme` (namcap), optdepends `gnome-keyring keepassxc` | |

On Ubuntu 22.04/Debian 12 `qml6-module-qtquick-controls` does not depend on the Templates, Window or
WorkerScript modules, so all are listed. The scanner on the Ubuntu image could not find QtQml
(`qml6-module-qtqml` is not part of the build image); the list above comes from the Fedora scan
plus the Ubuntu package names, and the clean-container installs prove it is sufficient.

### Sizes
| Artifact | Size |
|---|---|
| `fsync-linux` (release, stripped) | 39.0 MB (37 MiB) |
| `fsync-linux_0.1.0-1_amd64.deb` | 10.3 MB |
| `fsync-linux-0.1.0-1.x86_64.rpm` | 11.0 MB |
| `fsync-linux-0.1.0-x86_64.AppImage` | 44.9 MB (42.8 MiB; 78 MB of `usr/lib`, the ICU data is 29 MB of it) |
| Arch package (`pkg.tar.zst`) | 14 MB |

namcap (archlinux, 2026-11-07): `namcap PKGBUILD` clean (given a well-formed `_commit`; it cannot source
the placeholder); package: no errors. Remaining warnings, accepted: QML module `app.fsync` "uninstalled
dependency" (our own compiled-in module), unused `ld-linux`, glibc/libstdc++/libgcc implicit (now
declared), `qt6-wayland` "may not be needed" (the Wayland QPA is a run-time plugin, kept on purpose).
The earlier "Package contains reference to $srcdir" warning was NOT harmless: see fix round 1, M3.

### Install smoke matrix (clean containers, headless, 2026-11-07)
Each row: install without recommends, `--version`, `--build-info` has no features, `--smoke`,
`FSYNC_SMOKE_CORE=1 --smoke`, all libraries resolve, files installed (desktop, icons, license
report, mode 755), `desktop-file-validate` clean.

| Target | Artifact | Qt at run time | Result |
|---|---|---|---|
| ubuntu:22.04 | .deb | 6.2.4 | pass (12/12 checks) |
| debian:12 | .deb | 6.4.2 | pass (12/12) |
| ubuntu:24.04 (extra: proves the t64 alternatives) | .deb | 6.4.2 | pass (12/12) |
| fedora:latest | .rpm | 6.11.2 | pass (12/12) |
| ubuntu:22.04 without any Qt | AppImage | bundled 6.2 | pass (see fix round 1 for the corrected Wayland check) |
| archlinux:latest | PKGBUILD, local source | 6.12.0 | pass (12/12; build 15-17 min, `check()` incl. 124+ platform tests; package 14 MB) |

Negative tests: release `cargo build --features e2e-control` fails (`compile_error!` in
`fsync-linux-platform`); `FSYNC_ALLOW_TEST_FEATURES=1 build-release.sh` exits 1 in the guard before
Docker starts; `smoke-install.sh fedora <release binary with e2e-control>` exits 1 on `--build-info`
(`{"features":["e2e-control"],...}`).

| Check | Result |
|---|---|
| AppImage on a GNOME VM: QPA and Manual-mode shortcuts | pass with xcb, 2026-11-07 (Ubuntu 26.04, GNOME 50.1). Bundled Qt 6.2 picks xcb on GNOME by design ("Ignoring WAYLAND_DISPLAY on Gnome"); `QT_QPA_PLATFORM=wayland` also loads the bundled `libqwayland-generic.so` + EGL plugin (rc 0). Decision: keep xcb (XWayland) as the GNOME default (same as the Qt 6.2 .deb on 22.04); no AppRun override, because Qt 6.2 on GNOME Wayland draws its own decorations. Live with the AppImage's Qt 6.2 runtime (e2e binary swapped in for IPC; maps show bundled `libQt6Core`, `libqxcb.so`): Super+Alt+C sent the copied text to the CLI peer, Super+Alt+V then Ctrl+V pasted the CLI text; window focused without a click and auto-closed both times |
| Omarchy: `makepkg -si`, launcher entry, autostart toggle, relogin shows the tray | pending (manual) |
| Real Ubuntu 22.04 / Fedora desktops | untested (container smoke only) |

### Fix round 1 (code review 8.3 + tester, 2026-11-07)
- **H1/D2 Wayland check was blind.** `smoke-checks.sh` cut the QT_DEBUG_PLUGINS output with `head -8`
  and only looked for "Could not load", which a working plugin also prints without a compositor.
  Now the verdict uses the full output: PASS only on `Failed to create wl_display` (the plugin's code
  ran), FAIL on `Cannot load library .*qwayland` or `Could not find the Qt platform plugin`.
  Proven with negatives on the shipped AppImage (extracted copy, `AppRun` mode): wayland plugins
  deleted -> FAIL (`Could not find the Qt platform plugin "wayland"`); `libQt6WaylandClient.so.6`
  deleted -> FAIL; untouched -> PASS (log: scratchpad `f1-neg-wayland.log`).
- **M6.** AppImage extraction failure and a missing .desktop file are FAILs in every packaged mode
  (`env -u APPIMAGE_EXTRACT_AND_RUN` replaces the ineffective `=0`).
- **D1/M2 guard is an allowlist.** `release-guard.sh` lets every crate of this repo (core, proto,
  platform, app) enable only `SHIPPED_FEATURES` (none); any other feature fails closed (mutation:
  a default feature `x-test-bypass` on the platform crate -> exit 1). The test feature names stay
  refused anywhere in the graph. `--build-info` now merges `fsync_core::BUILT_FEATURES` (new, cfg-gated
  `test-hooks` / `test-relay-ca`) with the platform and app lists. `package-in-container.sh` fails if
  RUSTFLAGS / CARGO_*RUSTFLAGS is set or a cargo `config.toml` exists in CARGO_HOME or the repo. The
  PKGBUILD asserts exact version and empty features in `package()` (`--nocheck` cannot skip it) and
  clears RUSTFLAGS itself. Not exercised end to end: a core test feature actually showing up in a
  `--build-info` (it would need a test-feature app build).
- **M1/D4 provenance.** `git_sha` is `<sha>-dirty` for an unclean tree (computed on the host by
  `build-release.sh` and passed as `FSYNC_GIT_SHA`; `build.rs` falls back to git itself and also
  reruns on index changes). Dirty builds are allowed (nothing is committed yet);
  `FSYNC_RELEASE_STRICT=1` refuses them (verified: exit 1) and is the documented mode for real
  releases. Current artifacts report `c25c413-dirty`.
- **M3 vendored OpenSSL paths.** The vendored OpenSSL bakes `MODULESDIR`/`ENGINESDIR` of the build
  tree into the binary (`-D` string macros, which `-ffile-prefix-map` cannot rewrite; this was the
  `$srcdir` warning). Arch: `OPENSSL_NO_VENDOR=1` + `depends=(openssl)`: the package links the system
  OpenSSL (security updates through pacman, no build path). deb/rpm/AppImage: the path is
  `/src/apps/linux/target-ubuntu-2204/...` (root-only creatable on a user system) and `OPENSSLDIR`
  is `/usr/local/ssl`; accepted and documented, since an attacker would need root to create them.
  Not verified: whether anything loads an OpenSSL provider/engine by name. Consequence: the Arch
  package uses a different OpenSSL build than deb/rpm (open question for the owner). Manual step 9
  builds in `~/.cache/fsync-pkg`, never `/tmp`.
- **M4 AppImage on newer hosts.** `smoke-install.sh appimage-fedora` and `appimage-arch` run the same
  offscreen + wayland-plugin + license + desktop checks with base libraries only and no Qt: both pass
  (the bundled 22.04 glib/dbus/etc. start on Fedora latest and Arch). No GL driver/compositor is
  present, so interplay with host Mesa stays part of the GNOME LIVE row.
- **M5 PKGBUILD conventions.** `prepare()` runs `cargo fetch --locked` (all targets:
  `cargo about --offline` resolves the whole metadata graph; a first attempt with `--target` failed
  on `android_system_properties`), build/check use `--frozen`, `RUSTUP_TOOLCHAIN=stable` (the repo
  pin 1.99.0 belongs to the container builds; with rustup as cargo provider it would trigger a
  download mid-build). `cargo about generate --offline --locked --fail` in the PKGBUILD and in
  `package-in-container.sh`.
- **D3.** The AppImage ships `usr/share/doc/fsync-linux/THIRD-PARTY-LICENSES.txt`; smoke checks it.
- **Lows done:** artifact name never reaches a shell (fixed `/pkg/artifact`); exact `--version`
  match (script and PKGBUILD); `$CARGO_HOME` also remapped; PKGBUILD depends gained
  `glibc gcc-libs openssl`; `.SRCINFO` comment (generate without `FSYNC_LOCAL_SRC`); deb Qt lower
  bound `>= 6.2.4`; rpm `url`; `build-release.sh` removes `dist/` on failure; shared helpers in
  `scripts/lib/common.sh` (toolchain, qmlls.ini, git sha) used by three scripts; dead `OWNER` removed;
  namcap errors now fail the arch smoke (warnings are listed). Not done, follow-up: base image
  digests and rustup-init pinning, linuxdeploy trust-on-first-use (no upstream signature), SHA-1-only
  git pin / pacman git checksum, asset list still repeated per format (deb, rpm, PKGBUILD, smoke),
  rpm `license` (placeholder question), SHA256SUMS unsigned (phase 16).
- **Results:** ubuntu-2204, debian-12, ubuntu-2404 deb, fedora rpm, appimage (ubuntu:22.04,
  fedora, arch): all exit 0 after the round; arch PKGBUILD (local source, system OpenSSL 3.6.5, Qt 6.12.0): pass 12/12 + `PASS namcap: no errors`, package 13 MB, build 20 min + check 8 min; the makepkg `$srcdir` warning is gone. namcap warnings now also list `gcc-libs` "may not be needed" (kept: provides libgcc/libstdc++).

## Cross-OS results (phase 06)
Evidence, one run each (not one run for all five legs):
- `docs/test-results/linux-cross-os-261110-1241.json`: X1+X4+X3, git sha 312a67a, exit 0, 0 retries. Covers X3 and X4.
- `docs/test-results/linux-cross-os-261109-1753.json`: X1+X2+X5, git sha 994119f, all 12 legs sha256 equal. Covers X2 and X5.
- Earlier: `linux-cross-os-261109-1628.json` (X1, 5f54542), `linux-cross-os-261109-1741.json` (X1+X2, 994119f).
- `linux-cross-os-261110-1217.json` (ea954a6) is superseded. Its echo field was hardcoded and its skew probe gave negative latencies. Do not cite it for echo.

Harness: `apps/linux/tests/e2e/cross-os-with-windows.sh` (Linux side; `X3=1` adds the GUI legs, `X5=1` the relay leg).
Windows side: `apps/windows/tests/e2e/cross-os-cli-peer.ps1` (X2/X5, headless `fsync-cli`) and
`cross-os-gui-peer.ps1` (X3/X4, the WinUI app over UI Automation). See [apps/windows/README.md](../../apps/windows/README.md#cross-os-e2e-linux-app).

| Leg | Path | Kind | Latency (ms) | Limit (ms) | Note |
|---|---|---|---|---|---|
| X1 | Linux app -> fsync-cli (this host) | text | 97 | 5000 | run 1241, echo checked |
| X1 | fsync-cli -> Linux app | text | 81 | 5000 | echo checked |
| X1 | fsync-cli -> Linux app | file, 3 MB | 2160 | 5000 | accept scripted, hash equal |
| X1 | Linux app -> fsync-cli | file, 3 MB | 2157 | 5000 | pick-device, fetch, hash equal |
| X4 | Windows app pairs with Linux app | code (SAS) | - | - | SAS words equal; Windows lists the peer as Cli. QR text not tested |
| X3 | Windows app -> Linux app | text | 327 | 5000 | skew 8051 +-8 ms corrected; exact text |
| X3 | Windows app -> Linux app | html | 214 | 5000 | skew corrected |
| X3 | Windows app -> Linux app | png, 40x40 | 346 | 5000 | exact pixels |
| X3 | Windows app -> Linux app | file, 3 MB | 1391 | 10000 | eager inbox, hash equal |
| X3 | Linux app -> Windows app | text | 1367 | 5000 | skew 8054 +-8 ms corrected |
| X3 | Linux app -> Windows app | html | 140 | 5000 | skew corrected |
| X3 | Linux app -> Windows app | png, 40x40 | 311 | 5000 | exact pixels at the Windows clipboard |
| X3 | Linux app -> Windows app | file, 3 MB, lazy | 8701 | 10000 | Win+Alt+V, PasteHelper, hash equal. Thin margin (see Open) |

The 1753 run (X2, X5) used the older harness: skew probe (+-541 ms bound) and echo field (hardcoded). Its
numbers stand, but its echo claim predates the real checks.

| Leg | Path | Kind | Latency (ms) | Limit (ms) | Note |
|---|---|---|---|---|---|
| X1 | Linux app -> fsync-cli (this host) | text | 51 | 5000 | |
| X1 | fsync-cli -> Linux app | text | 79 | 5000 | |
| X1 | fsync-cli -> Linux app | file, 3 MB | 2239 | 5000 | accept scripted |
| X1 | Linux app -> fsync-cli | file, 3 MB | 2125 | 5000 | pick-device, fetch |
| X2 | Linux app -> fsync-cli on waso-vm | text | 2973 | 5000 | clock skew corrected, +-1127 |
| X2 | fsync-cli on waso-vm -> Linux app | text | 1067 | 5000 | clock skew corrected, +-1052 |
| X2 | Linux app -> waso-vm | file, 3 MB | 4256 | 30000 | hash equal (30 s gate in that run) |
| X2 | waso-vm -> Linux app | file, 3 MB | 4471 | 30000 | accept scripted, hash equal |
| X5 | Linux app -> waso-vm, `--force-relay` | text | 2706 | 5000 | clock skew corrected, +-395 |
| X5 | waso-vm -> Linux app, `--force-relay` | text | 424 | 5000 | clock skew corrected, +-410 |
| X5 | Linux app -> waso-vm, `--force-relay` | file, 8 MiB | 3229 | 30000 | hash equal (30 s gate in that run) |
| X5 | waso-vm -> Linux app, `--force-relay` | file, 8 MiB | 4478 | 30000 | accept scripted, hash equal |

Checks in the X3/X4 run (1241):
- Echo and duplicates: real counts on both sides. The Windows app published exactly the 4 copies it made, and the Linux app published 0 while receiving them. For the 3 Linux copies the Windows publish count did not change, and the Linux app published exactly 3. The lazy-file leg also left the Windows count unchanged.
- Pixels: coloured 40x40 PNG, distinct channels (grey would hide channel-order bugs). Alpha is not tested: CF_DIB carries none.
- Clock: the Windows clock is streamed over one ssh session. Error = TCP RTT + 6 ms. The leg fails when latency is below -error, and gates on latency + error <= limit.
- Gates: clips 5 s, Windows file legs 10 s. The gate is in `leg` in `cross-os-with-windows.sh`.
- Process (not run evidence): `taskMode` on waso-vm was set to interactive for the X3 run only, and restored to headless after both X3 runs on 2026-11-10.

Windows product bug found by X3 and fixed: Windows synthesizes CF_DIBV5 from a BITFIELDS CF_DIB by repeating the 12 mask bytes after a 124-byte header. The decoder did not skip them, so images were 3 px shifted. Fixed in `ImageCodec.cs` (`ea954a6`, `1e066a8`); see the [changelog](../project-changelog.md). Windows suite 173/173.

### Deviations and findings (owner decision needed)
- **File legs over Windows are gated at 10 s, not 5 s.** The spec gates clips at 5 s and file hash
  equality only; file legs over the ssh tunnel measure 3.2-5.0 s (one earlier X2 run hit 5014 ms). The
  recorded 1753 run used a 30 s gate (column above); the harness now gates file legs at 10 s (controller
  decision 2026-11-09). Text legs and all X1 legs keep 5 s.
- **Transport is a reverse SSH tunnel** (`ssh -R` opened by this host, waso-vm's own `127.0.0.1` forwarded
  back), not `netsh portproxy`. `cross-os-port-forward.ps1` only verifies it. Relay legs run over TCP, so
  QUIC address discovery is not forwarded. The tunnel lives for one run and leaves nothing on waso-vm.
- **A device of an existing account cannot join.** The Windows CLI must `pair offer`; the Linux app joins.
  The offer code reaches the host by reading a file over ssh (`%TEMP%\fsync-cross-os\pair-offer.out`),
  because `remote run` returns output only at the end.
- **OTP is not bound to the requesting client** (backend `otp.rs`: keyed by email and code hash, single use,
  5 min TTL). The host requests it, reads it from the dev backend log and passes it to the Windows run.
  `login --code` does not start a new OTP.
- Windows runs use **Windows PowerShell 5.1** (waso-vm has no pwsh 7). Builds need Strawberry Perl and NASM
  for vendored OpenSSL (`apps/windows/README.md`).
- Windows clocks run about 8 s ahead of this host (8051 ms in 1241, streamed probe; the one-shot probe measured
  about 8.4 s in 1217). The error is in each leg's `skew_err_ms` (+-8 ms in 1241).

### Known issues (pairing flakes)

| Flake | Seen | Symptom | Status |
|---|---|---|---|
| Local L1 <-> devB pairing (no Windows involved) | about 1 in 12 full runs | "Pairing failed: network: connection lost" on the joiner | Fixed 2026-11-11 (below) |
| Windows offer / Linux join | 2 of 5 attempts, once each | one run hung 120 s with no progress (an orphan Windows `pair offer` was left); another failed with "Can't reach the fsync server (No addressing information available)" | Open, likely cause below |

**Fixed: "connection lost" (2026-11-11, plan 261110 phase 01).** Not a relay or tunnel problem. The new
device (the side that offers) emitted `PairingEvent::Completed` right after writing the last frame (`Done`),
before the existing device had it. The CLI `pair offer` exits on `Completed`, and its engine shutdown cancels
the pair/1 handler, which drops the connection (implicit close, code 0, no reason). QUIC discards stream data
not yet sent, so the joiner, still waiting for `Done`, read "connection lost" (noq `ReadError::ConnectionLost`;
its source, "closed by peer: 0", was not shown). The list was already published and the new device had pinned
it: only the joiner reported a failure. This needs an offerer that tears its node down on `Completed`, which
the CLI does (the apps keep running).
- Reproduced with `apps/linux/tests/e2e/pair-flake-loop.sh` (fsync-cli <-> fsync-cli, fresh account per run):
  3 of 30 runs failed, plus 10 of 10 earlier passes (3 in 40). Each failure: joiner `pairing failed: network:
  connection lost` after `list_advanced seq 1` (list published), offerer `paired`, joiner log
  `conn::closed reason=ApplicationClosed { error_code: 0, reason: b"" }`, offerer `LocallyClosed` at the moment
  its router shutdown began (iroh "Shutting down remaining tasks" 0.6-0.7 s later, after `Endpoint::close`).
  Passing runs: the joiner closed first (`b"done"`).
- In-process, first test (round 1, 6184390): only partly a regression test. Its existing-device assert depends
  on timing: with the pre-fix responder it failed 7 of 40 rounds once on this host while it was loaded, then 0
  of 40 here and 0 of 192 in review. It did fail pre-fix every time, through its window-outcome assert (the
  dropped handler reported `shut down` in 38 to 40 of 40 rounds).
- Deterministic test since the fix round (`core/fsync-core/tests/pairing_done_delivery.rs`). Test hook
  `InitiatorHooks` (feature `test-hooks`) makes the existing device wait 300 ms after reading `Done` before it
  reports and closes. The test asserts that the existing device's `Completed` comes before the new device's.
  With the pre-fix responder in a scratch copy it failed 10 of 10 runs; with the fix it passes.
- Fix (`pair/responder.rs`): `Completed` is emitted after the drain, bounded by 5 s. The drain ends when the
  existing device closed the connection, which it does once it has read `Done` (log: `pair/1 responder drained
  closed=...b"done"`). A joiner that never closes gets `Completed` after 5 s instead (test: about 5 s).
  `Done` can then still be lost, but only if the joiner neither read it nor closed within 5 s.
- A node shutdown during the drain still emits `Completed`, and the window reports the pinned seq instead of
  `shut down` (drop guard and `PairTicket::set_pinned`). The test failed in about 5 of 6 runs without the guard:
  iroh cancels the handler, unless the endpoint close ends the drain first.
- Pair stream errors now carry their source chain (`CoreError::net_chain`, each source cut to 64 chars because
  the peer chooses its close reason). No retry added.
- Verified: the core tests (ordering, 5 s cap, shutdown during the drain); the loop 30 of 30 and 20 of 20 in round 1, and 20 of 20 again on the fix-round tree (gate 20). Cross-OS X1+X2+X5: not run yet.

**Known, not changed: the two sides can disagree.** The existing device publishes the list and ingests it
before it sends `SignedList`. If that frame or `Done` is lost afterwards (network loss, a joiner-side timeout of
15 s, or a new device that is killed), the existing device reports `Failed` while both sides already trust each
other (if the new device pinned). Nothing is left half-written. The device list is correct, and pairing again is
harmless: the existing device sees the node in its list and hands over the head without publishing. Possible
mitigation: on a loss after `SignedList`, re-check the backend's list and report success when the new device
is in it. Not done: it cannot tell whether the new device pinned, and a false "paired" is worse than a false
"failed". With the fix above, this needs a real network failure; teardown on `Completed` no longer causes it.

**Open: Windows "No addressing information available".** Not the same cause. The offer publishes the relay
URL it has after waiting at most 3 s for the home relay (`service/pairing.rs` `RELAY_WAIT`). iroh picks the
home relay only when its first net report ends, and over the TCP-only tunnel (and with the local test relay)
the QUIC address discovery probes get no answer, so that takes exactly iroh's `PROBES_TIMEOUT` of 3 s (measured
locally: bind to "home is now relay" 3.002-3.003 s in every run). An offer that loses this race carries no
relay URL; a joiner without mDNS (the tunnel) then has no address, which is iroh's `NoAddress` error, the
message seen. Not reproduced locally (the CLI starts the 3 s wait after the account check, so it wins here).
The offer now logs a warning when it has no relay URL. Candidate fix if X2 confirms it: wait longer than iroh's
5 s report timeout for the home relay. The 120 s hang is not explained.

An orphan Windows `pair offer` can linger up to 200 s after a failed pair run.

## Open
Nothing below blocks acceptance (the acceptance condition is X3 and X4, met above).
- **X4 QR text: not tested.** The phase 06 spec asks for "code and QR text"; only the code (SAS) path ran. Decide whether
  QR is needed before the Platform emit gate opens, or record it as a permanent gap.
- **Lazy file margin.** Linux -> Windows lazy file took 4577 ms (1217) and 8701 ms (1241) against 10 s. Variance comes
  from the Win+Alt+V poll (1.5 s steps) and PasteHelper. Not changed.
- **Offline account device delays publishes (product, not fixed).** Publishes are sequential. While one device of the
  account is offline, each following publish waits up to 7 s (about 2 s + 5 s tries). Affects the Windows app and the CLI.
- **X2 and X5 not re-run since the harness fixes.** Their evidence is at 994119f. The skew probe, echo checks and
  `tries` fields changed after that. Re-run `X5=1` at HEAD to refresh them.
- **Codec gaps.** 32-bpp images with non-standard masks are refused with a log line, not converted. Alpha is not tested.
  16-bpp masks other than 565/555 reach WIC unchecked.
- LIVE items carried from phases 02-05 and the Hyprland check (KeePassXC / Bitwarden not sent): tables above.
- Pairing flake on the Windows tunnel ("No addressing information available"): confirm the relay URL race
  (see "Known issues"). The local "connection lost" flake is fixed.
- Platform emit gate: the backend deploy and the Mac/Windows releases with the tolerant decoder.
- Confirm no tunnel process or portproxy rule remains on waso-vm and this host (phase 06 step 8). Not recorded yet.
