# Linux clipboard sync: watch/write, hotkeys, secrets, notifications, files

Date: 2026-11-06. Budget: 7 web lookups. Verified = source read. Unverified = memory/inference, flagged.

## 1. Wayland data-control

Protocols:
- `ext-data-control-v1`: staging in wayland-protocols, v1. Preferred.
- `zwlr-data-control-unstable-v1`: wlroots, v2+ needed for primary selection. Fallback.

Support (ext matrix, [wayland.app ext-data-control-v1](https://wayland.app/protocols/ext-data-control-v1)):

| Compositor | ext v1 | Impl version | Notes |
|---|---|---|---|
| Hyprland | yes | 0.52.1 | wlr also (per [lamco-data-control](https://docs.rs/lamco-data-control)) |
| Sway | yes | 1.11 | wlr also |
| KWin (Plasma 6) | yes | 6.7 | ext + wlr (lamco) |
| COSMIC | yes | 1.0.0~beta.8 | |
| niri, labwc, river, phoc, Treeland, Jay, Mir | yes | various | |
| GNOME Mutter | **no** | 51 | no ext, no wlr; lamco reports `Error::Unsupported`. Mutter issue [#524](https://gitlab.gnome.org/GNOME/mutter/-/issues/524) open (status unverified) |
| Wayfire, Weston, Cage, Muffin | no | | |

Rust:
- [wl-clipboard-rs](https://docs.rs/wl-clipboard-rs) has `watch::Watcher` (`new(clipboard, seat)`, `next_event()` blocks until change, cancel handle to unblock). Uses ext or wlr (any version for regular clipboard; ext or wlr v2+ for primary). Gives MIME types per event. Verified via docs/source.
- Cargo feature names and whether `copy` serves data from a background thread or fork: unverified, check crate source.
- Raw alternative: `wayland-client` + `wayland-protocols` staging `ext-data-control` (feature name unverified). More code, full control (own source, identity of offers).
- `lamco-data-control` (1.0.x) is a separate ext+wlr abstraction, young. Watch for abandonment risk.

Sketch (wl-clipboard-rs or raw):
1. Bind manager: prefer ext, else zwlr. If neither: GNOME path (sec 3).
2. Get device for seat. Subscribe to `data_offer`/`selection` events.
3. On selection event: record MIME list only (lazy). Read bytes only when needed, for allowed MIMEs, with timeout, in a thread.
4. Write: create our `data_source`, offer MIMEs, call `set_selection`. Keep process alive: data is served only while we own it.
5. Our source gets `cancelled` when another client takes selection. Stop serving; do not re-set. Treat as normal, not error.
6. Ignore primary selection (decided).
7. Our own writes echo back as selection event: drop by comparing to our source serial/ID, or by a write-hash guard to avoid loops.

Watch-out: data-control sees no source-client identity (no PID, no app-id). Denylist on Wayland must use MIME hints (sec 4) and content heuristics, not app identity.

## 2. X11

- Use `x11rb` with `xfixes` ext. `xfixes_query_version` then `xfixes_select_selection_input(root, CLIPBOARD, OWNER_CHANGE)`. Verified pattern in [yclip](https://docs.rs/crate/yclip/latest/source/src/clipboard/linux.rs) and [clip-bridge](https://docs.rs/crate/clip-bridge/latest/source/src/x11.rs). Both are small references, not libraries you depend on.
- On owner change: get owner window from event, `ConvertSelection` to `TARGETS`, pick MIME, convert. Handle `INCR` for large payloads (not covered by search; standard ICCCM, verify in x11rb examples).
- Own selection: create a hidden window, `SetSelectionOwner`, answer `SelectionRequest` events, handle `SelectionClear` (= lost ownership).
- Denylist by PID: read `_NET_WM_PID` on the owner window (may be unset; fall back to `/proc` via owner window `WM_CLIENT_MACHINE` or skip). Unverified: reliability for XWayland windows.
- Crates:
  - `arboard`: no watch API found in sources; old 0.1.1 source uses `xcb` not x11rb. Current backend unverified. Not recommended for watching.
  - `clipboard-rs`: x11rb backend with XFixes watcher (sources read on docs.rs). Possible, but it is a broader abstraction.
  - `x11-clipboard`: no watch, not checked. Skip.
  - Recommendation: write ~300 lines on x11rb directly, modeled on yclip/clip-bridge. Fits "Rust logic" and avoids dependence on an abstraction that does not expose owner window.

## 3. GNOME Wayland

- No data-control (Mutter 51 = No, [matrix](https://wayland.app/protocols/ext-data-control-v1)). Cannot watch in background.
- Qt `QClipboard` on Wayland: reads/writes work only with focused surface (Wayland clipboard rule, Unverified against Qt docs but well-known). Not a background watcher.
- XWayland fallback: Xwayland bridges Wayland selection to X selection, so XFixes owner-change fires for many Wayland copies. Viable for watch but unverified: whether events fire for all Wayland copies, latency, and whether reads from a non-focused X client work. Needs a test on GNOME 46-48 before committing.
- GNOME extension (Shell JS) is the alternative for a true watcher. Maintenance burden high, breaks per GNOME release. Not recommended.
- Practical answer: on GNOME, ship XWayland watcher as best effort; document degraded mode.

## 4. Sensitive-content hints

- `x-kde-passwordManagerHint` with value `secret`: set by KeePassXC when copying passwords. Klipper skips history for it, default since 2018 per [Clipman issue](https://gitlab.xfce.org/panel-plugins/xfce4-clipman-plugin/-/issues/72) and [KDE Phabricator](https://phabricator.kde.org/R120:637cd0bf62e419b8038cb9b1565089fbd341ee7a). CopyQ and DankMaterialShell reportedly respect it too (sources conflict on DMS detail; treat as Unverified).
- KDE bug [458063](https://bugs.kde.org/show_bug.cgi?id=458063): KDE Connect synced passwords copied from password managers. Direct precedent for our use case. Hint must be honored by us.
- Bitwarden: [community request](https://community.bitwarden.com/t/linux-prevent-passwords-from-being-stored-in-clipboard-manager-history/94231) to set the hint. Current behavior unverified; assume not set.
- 1Password Linux: unverified, assume not set.
- macOS convention `org.nspasteboard.ConcealedType` / `TransientType`: from memory, unverified here. Harmless to honor too.
- Rule: if any hint present, do not sync and do not store in history. On receive from peers, set hint on our write when content is marked secret, so local managers skip.
- Clear-on-timeout is not covered by any hint. Our app should clear its own offers after N seconds for hinted items (our own write, so we control it).

## 5. Global shortcuts

- `org.freedesktop.portal.GlobalShortcuts`. Per an undated search snippet (source unclear, [xfwl4 issue 93](https://gitlab.xfce.org/xfce/xfwl4/-/issues/93) listed in same results): implemented by gnome, kde, hyprland portals; gtk, wlr, xapp, cosmic did not. Treat as snapshot.
- KDE Plasma: works; app proposes triggers, user confirms in KDE dialog ([blog 2024](https://dec05eba.com/2024/03/29/wayland-global-hotkeys-shortcut-is-mostly-useless/)).
- Hyprland: supported since 0.24.1; ignores app default triggers; list with `hyprctl globalshortcuts`. Verified via search snippet, wiki at [wiki.hypr.land](https://wiki.hypr.land/configuring/core/binds/globals/).
- GNOME: a niri PR comment (Feb 2025) forecast GNOME 48 support. Not confirmed. Verify in xdg-desktop-portal-gnome changelog.
- Sway/niri/COSMIC: not supported by portal per snapshot. Unverified for 2026.
- Crate: `ashpd` wraps `GlobalShortcuts` (sources on [docs.rs](https://docs.rs/crate/ashpd/0.9.3/source/src/desktop/global_shortcuts.rs)). Pick latest version; check changelog.
- Fallback (all targets, recommended as the baseline): app runs CLI/IPC endpoint (Unix socket). Users bind in compositor config: Hyprland `bind = SUPER, V, exec, <bin> toggle`, Sway `bindsym $mod+v exec <bin> toggle`. Works everywhere, zero portal risk. Ship this first.

## 6. Secrets

- Secret Service API (`org.freedesktop.secrets`) via gnome-keyring, KeePassXC, KWallet (KDE bridge). Minimal Hyprland/Sway setups often have none.
- `keyring` crate: `sync-secret-service` feature (feature name verified in memory, not in sources). Fails at runtime if no provider on bus (error type not verified; my memory: `NoStorageAccess` / platform failure).
- `oo7`: pure Rust, zbus-based. Two backends: Secret Service D-Bus, and encrypted file (libsecret-compatible). Docs recommend file backend for sandboxed apps. Source: [oo7 docs](https://docs.rs/oo7). Active (18 releases). Pre-1.0 (0.1.0-alpha.x history): API churn risk.
- secret-service-rs: sync only, stale per oo7 docs. Skip.
- Recommended order:
  1. At startup check `org.freedesktop.secrets` on session bus. Present: use oo7 Secret Service backend.
  2. Absent: offer to install gnome-keyring (or enable KeePassXC Secret Service). Do not store in plaintext.
  3. Fallback: oo7 file backend, key derived from user passphrase at launch. Clear error if user declines.
- Never log secrets. Use `zeroize` for in-memory key material (standard practice, unverified here).

## 7. Notifications

- `notify-rust`: default zbus backend in v4+ (memory, unverified in this session). Talks `org.freedesktop.Notifications`, which exists on all targets (GNOME Shell, KDE, mako, dunst, swaync).
- Qt: `KNotification` (KDE Frameworks) or QtDBus. Adds Qt/KF dependency for no gain when logic is Rust.
- Recommendation: `notify-rust` from Rust side. One path, no Qt dependency.

## 8. Files on clipboard

- Plain `text/uri-list`: `file://` URIs, CRLF-separated (RFC 2483). No cut/copy flag.
- `x-special/gnome-copied-files`: first line `copy` or `cut`, then URIs. Nautilus, Thunar. Newer Nautilus (GTK4 era) also uses `x-special/nautilus-clipboard` ([Nautilus history](https://lists.gnome.org/archives/nautilus-list/2008-June/msg00022.html), [GNOME commits 2022](https://mail.gnome.org/archives/commits-list/2022-January/msg01303.html)). No formal spec (per Rhythmbox 2010 bug, older source).
- KDE: `application/x-kde-cutselection` for cut. Reliability reported as inconsistent.
- Dolphin accepts plain `text/uri-list`; Nautilus historically did not (search snippet from a spice-gtk / vd_agent MR discussion, [vd_agent MR 13](https://gitlab.freedesktop.org/spice/linux/vd_agent/-/merge_requests/13); unverified).
- Recommendation: offer `text/uri-list` + `x-special/gnome-copied-files` (copy) + `x-kde-cutselection` (0). Files must exist locally: remote file bytes need transfer first. Scope this out of v1 (YAGNI) unless required.

## Ranked choices

1. Wayland watch/write: `wl-clipboard-rs` watcher (ext preferred, auto fallback to zwlr). Upgrade to raw `wayland-client` only if its `copy` cannot keep source identity or cancel semantics we need. Fit: Rust-only, covers Hyprland, Sway, KWin, COSMIC.
2. X11 watch/write: raw `x11rb` + XFixes, modeled on yclip/clip-bridge. Not arboard.
3. GNOME: XWayland watcher as best effort. Test before promising. Ship degraded-mode UI message.
4. Hotkeys: CLI/IPC fallback first (universal). Add `ashpd` GlobalShortcuts for KDE and Hyprland as enhancement.
5. Secrets: oo7 (Secret Service, file fallback). Reject plaintext.
6. Notifications: `notify-rust`.
7. Hints: honor `x-kde-passwordManagerHint` and `org.nspasteboard.ConcealedType` (unverified convention, low cost).

## Trade-offs (watch backend)

| Option | Perf | Complexity | Maintenance | Coverage | Risk |
|---|---|---|---|---|---|
| wl-clipboard-rs watcher | good (blocking loop) | low | 3rd-party, moderate activity | ext+wlr | abstraction hides owner identity |
| raw wayland-client | good | high (~800 LOC est.) | ours | ext+wlr | protocol staging churn |
| Qt QClipboard | good | low | Qt-owned | focused only | no background watch |
| GNOME Shell extension | good | high | breaks per GNOME release | GNOME only | high |
| XWayland watch | good | low (reuse x11rb) | ours | GNOME/any Xwayland | event coverage unverified |

## Adoption risk

- wlr-data-control: long-lived, de facto standard. Low abandonment risk. Migration to ext is a rename-level change.
- ext-data-control: staging, "testing phase". Breaking changes possible until stable. Track wayland-protocols releases.
- oo7: pre-1.0, active. Medium churn risk.
- wl-clipboard-rs: single-maintainer-ish. Medium risk. Pin version.
- ashpd: mature, used by Flatpak ecosystem. Low.
- notify-rust: mature. Low.

## Architectural fit

- Rust logic + Qt/QML UI: keep all platform I/O in Rust. Qt only for UI. Avoids QClipboard limits entirely.
- Dev machine is Hyprland: primary target, best-supported path (ext + portal + wlr all present).
- Team skill: Rust-centric; x11rb/wayland-client are idiomatic.
- No prod hosts involved. No deploy constraints.

## Limitations

- Did not verify GNOME 48+ portal support, Bitwarden/1Password hints, or XWayland event coverage on GNOME. These need a live test matrix.
- Did not read wl-clipboard-rs or oo7 source for feature flags and error types. Only docs snippets.
- Compositor matrix is a snapshot from a third-party site; confirm on target versions.
- No benchmarks. Latency and CPU on large payloads (images) not measured.
- No security review of hint-based denylist; hints are advisory, any app can omit them.

## Unresolved questions

1. Does wl-clipboard-rs `copy` keep serving after the calling process returns (fork/thread)? Needed to decide raw vs crate.
2. GNOME 48/49/50/51: is the GlobalShortcuts portal shipped in xdg-desktop-portal-gnome? Is Mutter data-control planned?
3. Does XWayland selection bridge fire XFixes owner-change for all GNOME Wayland copies? Need test.
4. Do Bitwarden/1Password set any hint on Linux in 2026?
5. Sway/niri GlobalShortcuts portal status in 2026: unsupported or implemented?
6. Exact `keyring` error variant when no Secret Service provider: confirm in source before writing fallback.
7. Is file clipboard in scope for v1? Requires remote file transfer design.

Status: DONE_WITH_CONCERNS
Summary: Ext-data-control is the Wayland target (Hyprland, Sway, KWin 6.7, COSMIC, niri); GNOME Mutter has no data-control, so GNOME is degraded (XWayland best-effort). X11 via x11rb XFixes directly. Hotkeys: CLI fallback first, portal second. Secrets: oo7 with passphrase file fallback. Concerns: GNOME portal/XWayland coverage and wl-clipboard-rs internals unverified; need live test on Hyprland, Sway, KDE 6, GNOME.
