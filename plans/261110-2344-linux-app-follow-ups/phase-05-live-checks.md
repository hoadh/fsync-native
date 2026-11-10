# Phase 05 — LIVE checks (Hyprland host + GNOME VM) and tunnel-leftover check

## Context
Open LIVE items carried from plan 261106:
- phase-02:98 — Omarchy: tray clicks, real login, QR, Wayland raise.
- phase-02:111 — GNOME VM without AppIndicator: no tray, window reachable via launcher (IPC show); Secret Service via gnome-keyring; a relaunch after keyring unlock keeps the node id.
- phase-03:98 — Hyprland: a KeePassXC / Bitwarden copy is not sent (password-manager filter).
- phase-04a:73 — GNOME VM: paste received files in Nautilus.
- Drag-drop send (04b), on Hyprland and GNOME.
- phase-06:82 — no tunnel process left on waso-vm or this host after a cross-os run.

## Overview
- Priority P2. About 1 d with the user.
- The user runs these by hand on their own desktop (I never drive the real clipboard, keyring or session).
- I prepare a checklist, set up the counterpart device (fsync-cli in a throwaway data dir) and read the evidence.

## Steps
1. Write `docs/how-to/linux-live-checks.md`: one section per item, with the exact steps, the expected result, and what to send back (a screenshot or `fsync-linux --build-info` + log lines).
2. **Counterpart:** I start `fsync-cli` against staging or the local stack with a temp data dir, pair it with the user's app, and watch what arrives. For the password-manager item that means nothing must arrive within 10 s.
3. **GNOME VM:** items that need a real session are done by the user over its desktop. Over ssh I may only read logs and `--build-info`.
4. **Tunnel check** (automatic, mine): after the phase-01 or phase-03 cross-os run, check `ps` on this host and `Get-Process ssh,fsync*` on waso-vm. Record the result in the phase-06 file of plan 261106.
5. Record each result in the ADR 014 LIVE table (pass / fail / untested, with the date). A failure becomes its own fix task. It does not block the close-out if it is in a known-untested area.

## Todo
- [ ] Checklist doc
- [ ] Hyprland: password manager, tray/login/QR/raise, drag-drop
- [ ] GNOME VM: no-tray launcher, keyring relaunch, Nautilus paste, drag-drop
- [ ] Tunnel leftover check recorded
- [ ] ADR 014 LIVE table updated

## Risks
Needs the user's time at both desktops. Items they skip are recorded as "untested" (allowed by plan 261106's cross-cutting rules).
