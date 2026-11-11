# How to run the Linux app LIVE checks

These checks need a real desktop session, so a person runs them by hand. The automated e2e cannot
cover them, because it never touches a real clipboard, keyring or tray. Each result goes into the
ADR 014 tables (`docs/adr/014-linux-desktop-app.md`) as pass, fail or untested, with the date.

## Before you start

1. Install the app from `main`, so the checks cover the final code. On Omarchy, follow
   "Arch PKGBUILD" in `apps/linux/README.md` (tarball + `FSYNC_LOCAL_SRC`, built in
   `~/.cache/fsync-pkg`). Then run `fsync-linux --build-info`. `git_sha` must be the `main` commit and
   `features` must be `[]`.
2. **Counterpart device.** Several checks need a second device on the same account to send to or
   receive from. Use one of these:
   - one of your own devices (Mac, Windows or a second Linux machine); or
   - an `fsync-cli` in a throwaway data dir that you sign in with your account and pair with the app.
     Remove it afterwards in the app: Devices > the device > Remove.
3. **Debug log (optional):** quit the app from the tray, then start it as
   `FSYNC_LOG=info fsync-linux 2> ~/fsync-live.log`. The log goes to stderr.

For each check, send back what the "Evidence" line asks for.

## Hyprland (Omarchy host)

### H1. Install, launcher, autostart, tray after relogin
- **Steps:**
  1. `makepkg -si` (see above).
  2. Open "fsync" from the launcher.
  3. Settings > turn autostart on.
  4. Log out and back in.
- **Expected:** the app starts in the background, and the tray icon shows after login with no
  window opening. `~/.config/autostart/app.fsync.Fsync.desktop` exists.
- **Evidence:** a screenshot of the tray icon, and `ls ~/.config/autostart/`.

### H2. Tray clicks
- **Steps:** open the tray menu and click, in this order:
  1. Open fsync
  2. History
  3. Send clipboard now
  4. Pause sync (then click it again to resume)
  5. Quit
- **Expected:**
  - Open fsync and History each raise the window on the right tab.
  - Send clipboard now makes the counterpart receive the current text.
  - Pause sync gets a check mark, and while it is checked nothing is sent.
  - Quit ends the process: `pgrep -a fsync-linux` prints nothing.
- **Evidence:** pass/fail per item.

### H3. Real login and QR pairing
- **Steps:**
  1. Sign out in the app.
  2. Sign in with your email and the code you receive.
  3. Pair a counterpart that can scan a QR code (Mac or Windows app), or use the code words.
- **Expected:**
  - The QR code renders sharply and can be scanned.
  - The SAS words are the same on both screens.
  - The device then shows in Devices.
- **Evidence:** pass/fail, and which counterpart you used.

### H4. Wayland raise
- **Steps:**
  1. Put the fsync window behind another window, or on another workspace.
  2. Run `fsync-linux --show` in a terminal.
- **Expected:** the existing window comes to the front, and the command exits in under 1 s.
- **Evidence:** pass/fail.

### H5. Password manager copy is not sent
- **Steps:**
  1. Copy a password in KeePassXC.
  2. Copy a password in Bitwarden (desktop app or browser extension).
  3. Wait 10 s each time.
- **Expected:** the counterpart receives nothing, and nothing new shows in History.
- **Note:** a manager that sets no password hint and is not in the denylist can still be synced
  (see README "Privacy"). Record which one.
- **Evidence:** pass/fail per manager, plus the matching lines from `~/fsync-live.log` if you have it.

### H6. Manual-mode hotkey and settings after a restart
- **Steps:**
  1. Settings > Clip mode = Manual.
  2. Add `bind = SUPER ALT, C, exec, fsync-linux --clip-send` to `~/.config/hypr/bindings.conf`.
  3. Copy some text and press Super+Alt+C.
  4. Quit the app and start it again.
- **Expected:** the counterpart receives the text, and after the restart Clip mode is still Manual.
- **Evidence:** pass/fail.

### H7. Received files paste in a file manager
- **Steps:**
  1. Copy a file on the counterpart.
  2. On this host, paste in Nautilus or Thunar.
- **Expected:** the file is created with the right content.
- **Evidence:** pass/fail and the file manager name.

### H8. Drag and drop send
- **Steps:**
  1. Drag a file from the file manager onto a device row in the fsync window.
  2. Drag another file onto an empty part of the window.
- **Expected:**
  - The first drop sends to that device at once.
  - The second drop opens the device picker.
  - The counterpart receives both files.
- **Evidence:** pass/fail per drop, and the file manager name.

## GNOME VM (`ubuntu-gnome`, no AppIndicator extension)

### G1. No tray, window reachable from the launcher
- **Steps:**
  1. Start fsync from Activities.
  2. Close the window.
  3. Start it from Activities again.
- **Expected:**
  - No error about a missing tray.
  - Closing the window minimizes it (with no tray there is nothing to hide to).
  - The second start shows the existing window (IPC show); it does not start a second process.
- **Evidence:** pass/fail, and `pgrep -a fsync-linux` (one line).

### G2. Keyring, and a relaunch keeps the node id
- **Steps:**
  1. Sign in.
  2. Note the node id shown in Settings.
  3. Log out of GNOME, log in (the keyring unlocks at login), and start fsync.
- **Expected:** the app is still signed in with the same node id, and nothing asks to pair again.
- **Evidence:** both node ids.

### G3. Received files paste in Nautilus
- **Steps:**
  1. Copy a file on the counterpart.
  2. On the VM, run `fsync-linux --clip-paste` (or use the shortcut), then press Ctrl+V in Nautilus.
- **Expected:** the file is created with the right content.
- **Evidence:** pass/fail.

### G4. Drag and drop from Nautilus
- **Steps:** as H8, but drag from Nautilus.
- **Expected:** as H8.
- **Evidence:** pass/fail per drop.

## After the checks
- Remove the counterpart CLI device if you added one.
- Turn off anything you changed only for the test: autostart, Manual mode, the Hyprland binds.
