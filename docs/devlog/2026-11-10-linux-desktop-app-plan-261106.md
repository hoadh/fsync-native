# Linux desktop app v1 (plan 261106)

**Date**: 2026-11-10 19:58
**Severity**: Medium
**Component**: `apps/linux` (Qt6/QML over CXX-Qt), `core/fsync-proto` (Platform codec)
**Status**: Shipped

## What Happened

The Linux desktop app merged to `main` as PR #1 (merge `4e7b33b`). It covers sign-in, pairing (code, QR, SAS), devices, clipboard sync on Wayland data-control, X11 (XFixes) and a GNOME manual mode, eager file inbox up to 20 MiB, history, settings, drag-drop and `--send`, tray, and autostart. Packages (.deb, .rpm, AppImage, PKGBUILD) were smoke-tested in clean containers. Cross-OS legs X1, X3 and X4 passed against `fsync-cli` and the Windows app on waso-vm (`docs/test-results/linux-cross-os-261110-1241.json`). ADR 014 is accepted. The plan ran 7 phases, about 29 working days estimated.

## Impact

Linux joins macOS and Windows as a full client. Two decisions shape what users get: clipboard sync on GNOME is not automatic (it needs a hotkey and a focused window), and the app sends `Platform::Cli` on the wire, because peers do not yet understand `Linux`.

## Technical Details

- Tree: `apps/linux` is its own Cargo workspace (`platform` without Qt, `app` with Qt), so root `cargo test --workspace` never needs Qt. `scripts/check-lock-drift.sh` pins fsync-core's dependency tree to the root lock (291 crates).
- Wayland: raw `wayland-client` 0.31 plus `ext-data-control-v1` / `zwlr`. `wl-clipboard-rs` 0.9.5 was rejected in a spike: a paster that never reads blocks `serve()`, and that cannot be cancelled at shutdown.
- Distro floor: Ubuntu 22.04 / Debian 12 build (`GLIBC_2.34`). One release binary feeds all three formats.
- Phase 06 cross-OS legs: X3 text Windows -> Linux 327 ms, Linux -> Windows 1367 ms, lazy file 8701 ms (gate 10 s).

## What We Tried

- Per-distro builds were rejected once the Qt 6.2 binary ran `--smoke` on Fedora (Qt 6.11.2) and Arch (Qt 6.12.0).
- X11 INCR send and `MULTIPLE` were not implemented; oversize entries give up the selection instead.
- Default-Manual on Wayland was rejected for the sensitive-content filter: it would switch off sync on every Wayland desktop to protect against password managers that set no hint.

## Key Decisions

- **Rust + CXX-Qt, no FFI layer.** `uniffi-bindgen-cpp` supports only uniffi 0.28.3; the repo pins 0.29.4 (ADR 008). CXX-Qt is pinned exactly (`=0.10.0`).
- **Platform emitted as Cli.** Decoding is tolerant (`fsync-proto` `platform.rs`); emitting `Linux` waits for the backend deploy and Mac/Windows releases with the tolerant decoder. The `emit-platform-linux` feature is off.
- **Files are eager.** Lazy reps were dropped (red team finding 10). Inbound is always fully fetched before the entry is written.
- **Lifecycle v1:** the owned clip is served by our process, so it disappears on quit. No clipboard-manager hand-off.
- **Trade-off accepted:** the sensitive filter cannot identify the source on Wayland (data-control carries no identity). Password managers that set no hint and are not focused are synced.

## Lessons Learned

- Red team review on the plan (15 findings, 3 critical) changed the design before code: the `Platform` wire break and the Option A port-forward for cross-OS testing were both caught there. Keep that step for protocol changes.
- Several "pass" items were partial. The GNOME, Hyprland and cross-OS results were recorded per leg with evidence; keep that format for live checks.
- The password-manager check was deferred for lack of a manager on the test host. Say so in the plan, not only in the ADR.

## Next Steps

- Platform emit gate: backend deploy plus Mac/Windows releases carrying the tolerant decoder (owner: release owner, no date set).
- X4 QR-text pairing was never tested; decide whether it is required before the gate opens (owner: Alex).
- KDE Plasma, Sway live session and real Ubuntu 22.04 / Fedora desktops are untested (ADR 014 "Untested desktops").

## Unresolved Questions

- Arch uses system OpenSSL, deb/rpm/AppImage use vendored OpenSSL. Nothing verified whether an OpenSSL provider is loaded by name.
- The first send after about 30 s idle costs about 2 s (a stale QUIC direct path waits its full `FIRST_ATTEMPT`). Not changed.
