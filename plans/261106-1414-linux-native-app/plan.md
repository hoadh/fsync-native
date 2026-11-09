---
title: "fsync Linux desktop app (Qt6/QML + CXX-Qt)"
description: "Linux tray/desktop app at feature parity with Windows phases 13+14: Rust + CXX-Qt over FsyncService, Wayland/X11 clipboard, files, history, packages, cross-OS test."
status: in-progress
priority: P2
branch: main
tags: [feature, linux, desktop]
blockedBy: []
blocks: []
created: 2026-11-06
---

# Goal
`apps/linux`: Qt6/QML UI, Rust QObjects (CXX-Qt) calling `fsync_core::service::FsyncService` directly (no FFI). Parity with Windows app (ADR 013) minus "Send to" shell menu. Branch `feat/linux-app`, not `main`.

Inputs: [brainstorm](../reports/brainstorm-261106-1354-linux-native-app.md), [research: CXX-Qt + packaging](../reports/researcher-261106-1354-cxx-qt-and-linux-packaging.md), [research: clipboard, hotkeys, secrets](../reports/researcher-261106-1354-linux-clipboard-hotkeys-secrets.md), [red team](reports/red-team-261106-1438-plan-review.md). Parity: `docs/adr/013-windows-clipboard-app.md`, `apps/windows/README.md`.

# Phases
| Phase | Name | Status |
|---|---|---|
| [01](phase-01-spike-and-scaffold.md) | Spike (CXX-Qt on Qt 6.2, cross-distro run, go/no-go) + scaffold + GNOME VM + ADR 014 draft (3 d) | Completed |
| [02](phase-02-engine-bridge-and-app-shell.md) | Tolerant `Platform` decode, engine bridge, single instance + IPC, tray, sign-in, pairing, devices, secrets, autostart (7 d) | In Progress (LIVE checks open) |
| [03](phase-03-clipboard-backends.md) | Clipboard: Wayland data-control, thin X11, GNOME manual window, filters, eager inbound, lifecycle (6 d) | In Progress |
| [04a](phase-04a-files-and-history.md) | Eager file inbox, retention, offers, history copy-back (3 d) | In Progress (code done, review 9.5; LIVE checks open) |
| [04b](phase-04b-settings-drag-drop-cli-hotkeys.md) | Settings, drag-drop send, CLI hotkey binds (2 d, below MVP cut line) | In Progress (code done, review 9.5; manual Omarchy checks open) |
| [05](phase-05-packaging-and-distro-smoke.md) | PKGBUILD, .deb, .rpm, AppImage; release guard; install smoke (4 d) | Completed (review 9.5; GNOME AppImage xcb LIVE + Omarchy makepkg/autostart pass) |
| [06](phase-06-cross-os-test-and-docs.md) | Cross-OS (fsync-cli, Windows app on waso-vm via port-forward) + docs, ADR 014 final (4 d) | Pending |

Progress: 0/7. Estimate ~29 working days (single dev).

# Dependencies
- Sequential: 01 -> 02 -> 03 -> 04a -> 04b -> 05 -> 06. 03/04a/04b all touch `app/src/bridge/mod.rs`, `main.rs`, `Main.qml`, `tray.rs`: no parallel run.
- 04b stays in v1 (validation 2); the cut line only applies if the schedule forces it (then 05 depends on 04a, SettingsView stub).
- 06 needs: user reinstalls Rig on waso-vm, `remote setup waso-vm` + `doctor` pass.
- MVP plan `plans/261013-2029-fsync-native-mvp`: phases 15/16 neither block nor are blocked. Linux signing/repos belong to 16.

# Gates
- **Qt floor go/no-go** (phase 01): CXX-Qt builds on Qt 6.2 (ubuntu:22.04); the 6.2 binary runs `--smoke` in `fedora:latest` + `archlinux:latest` containers (qmlcachegen/Qt version coupling); decide one binary vs per-distro builds; decide AppImage Qt (6.2 apt vs pinned 6.8 LTS via aqtinstall). No-go on 6.2 -> 6.4 floor; no-go on both -> bundle pinned Qt 6.8 LTS (aqtinstall) in AppImage and in .deb/.rpm under `/opt` (decided, validation 2).
- **GNOME VM provisioned** (phase 01): user's existing Ubuntu Desktop (GNOME Wayland) machine reachable over ssh before phase 03 starts.
- **Platform emit gate** (future release, out of scope): Linux app may emit `Platform::Linux` only after prod backend + Mac/Windows app releases with the tolerant decoder are deployed and adopted. Recorded in ADR 014.
- **Cross-OS transport** (phase 06): waso-vm reaches backend + relay via port-forward to its own `127.0.0.1` (`ssh -L` or `netsh portproxy`); `doctor` green.
- **Test-feature release guard** (phases 02 + 05): `e2e-control` in a release build is a compile error; packaged binaries checked via `--build-info`.

# Cross-cutting rules
- Source files < 200 lines (`docs/code-standards.md`), Rust `snake_case.rs`, QML `PascalCase.qml`, scripts `kebab-case`.
- Core stays Qt-free. `apps/linux` is its own Cargo workspace: root `cargo test --workspace` never needs Qt.
- Engine calls never on the Qt GUI thread; callbacks reach QML only via `CxxQtThread::queue`.
- `e2e-control` never in packaged builds (`compile_error!` + `--build-info` check, see Gates).
- No CI workflow edits in this plan (Actions billing block); local container scripts are the gate.
- LIVE items only for Hyprland (host), GNOME VM, waso-vm; other desktops -> ADR 014 "untested" list.
- Never log secrets, tokens, clipboard content or file names.

# Success (whole plan)
- Packages install + `--smoke` pass on Arch, Ubuntu 22.04, Debian 12, Fedora (containers) and Omarchy host.
- Hyprland: auto sync text/html/png both ways with fsync-cli and Windows app on waso-vm, each <= 5 s; file round trip hash match.
- GNOME VM: manual send/paste via mini window works (LIVE).

# Decisions (validation 2026-11-06, revised after red team)
- Secrets fallback without Secret Service: 0600 file + visible warning; backend choice persisted in marker file, never re-detected (phase 02).
- Platform, two steps: (1) this plan makes `Platform` decoding forward-compatible in fsync-proto (unknown index preserved, treated as Cli) and adds `Linux` that no build emits; Linux app sends `Cli` on the wire (feature `emit-platform-linux`, default off). Backend gets the tolerant decoder in its next regular deploy. (2) Emitting `Linux` = later release behind the Platform emit gate.
- Cross-OS network: Option A, dev backend + relay on this host, port-forwarded so every peer sees `127.0.0.1`; relay over TCP only, no app http override.
- waso-vm Windows GUI: Rig interactive task (`taskMode: interactive` is machine-wide; dev user logged on).
- Inbound clipboard always eager; X11 thin (no INCR send); no portal shortcuts, no FileManager1, no notification-action download.

# Unresolved questions
- **Rig mid-run output**: does `remote run` stream stdout so Linux can read a Windows pairing code mid-run? If not, Linux offers and passes `-PairCode` (phase 06; verify at phase 06 start).

## Validation Log
### Session 2 — 2026-11-06
| Question | Answer | Propagated |
|---|---|---|
| GNOME test host | User's existing machine: Ubuntu Desktop (GNOME); no VM provisioning | plan Gates, phase 01 (env, step 13) |
| CXX-Qt fails on Qt 6.2 and 6.4 | Bundle pinned Qt 6.8 LTS (aqtinstall): AppImage + .deb/.rpm under `/opt` | plan Gates |
| Fedora versions | Latest only | phase 05 smoke matrix |
| App id | `app.fsync.Fsync` | unchanged |
| MVP cut (04b) | Keep 04b in v1 (05 depends on 04b) | plan Dependencies |

## Red Team Review
### Session — 2026-11-06
**Findings:** 15 (15 accepted, 0 rejected; 2 sub-suggestions rejected)
**Severity breakdown:** 3 Critical, 11 High, 1 Medium

| # | Finding | Severity | Disposition | Applied To |
|---|---|---|---|---|
| 1 | `Platform::Linux` wire break | Critical | Accept, two steps (tolerant decode now, emit later) | plan Decisions/Gates, 02, 06 |
| 2 | Secrets re-detection creates new keys; blocking persist on callback thread | Critical | Accept | 02 |
| 3 | Single-instance lock/socket races, `/tmp` squat, clip cmds signed out | High | Accept | 02 |
| 4 | Option A blocked by loopback-only http / test CA | Critical | Accept: port-forward, TCP relay | plan Decisions/Gates, 06 |
| 5 | `remote run` blocking: no barrier, OTP, TTL, taskMode | High | Accept | 06 |
| 6 | Release guard blind to `e2e-control`; PKGBUILD; SAS bypass | High | Accept | plan Gates, 02, 05 |
| 7 | GNOME manual mode undesigned | High | Accept | plan Gates, 01, 02, 03 |
| 8 | uri-list confused deputy | High | Accept partly (folders still allowed) | 03, 04a |
| 9 | Sensitive filter fails open | High | Accept partly (no default Manual on Wayland) | 03 |
| 10 | Lazy reps + prefetch = eager twice | High | Accept: eager only | 03 |
| 11 | No clipboard exit/lifecycle policy | High | Accept | 02, 03 |
| 12 | Serial `engine_worker` blocks Cancel/Quit | High | Accept | 02, 03 |
| 13 | qmlcachegen tied to Qt version | High | Accept | plan Gates, 01, 05 |
| 14 | Scope bloat, LIVE without hardware, no estimates | High | Accept | plan, 01, 03, 04a, 04b, 05, 06 |
| 15 | Env/tooling gaps, stale refs | Medium | Accept | plan, 01, 02, 03, 05 |
