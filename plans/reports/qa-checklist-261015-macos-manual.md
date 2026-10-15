# macOS manual QA checklist (owner)

Covers what automation cannot: ADR 009 paste matrix, phase 04/12 manual items, Mac<->Windows GUI pairing,
the privacy alert, password managers. Tick each box; write the macOS version, build and result in the
Result column or margin. Items marked MUST gate release (phase 15: 100% of must-items on macOS N and N-1).

## 0. Setup

| # | Step | Expected |
|---|---|---|
| 0.1 | Build the app: `bash apps/macos/scripts/build-fsync-mac-app.sh --api https://api-fsync.10z.one --relay <relay1>,<relay2> --out /tmp/fsync-qa` (add `--sign "Developer ID Application: Hoa Dang (VE5P5RS83W)" --profile <file> --notary-profile fsync-notary` for a signed run). | `FsyncMac.app` in the out dir, build succeeds |
| 0.2 | Reset state for a clean run: quit fsync, then Keychain Access > search `app.fsync.mac` > delete items; `rm -rf ~/Library/Application\ Support/app.fsync.mac` | next launch shows "Sign in to fsync" |
| 0.3 | Have ready: a Windows 10/11 PC with the fsync Windows app, 1Password (or Bitwarden), Keychain Access, Safari with a saved password, Finder, Mail, Messages, Slack, VS Code, Chrome. Two macOS versions if possible (N = 26.x, N-1 = 15.x). | - |
| 0.4 | Create test files: `mkdir -p ~/fsync-qa && cd ~/fsync-qa && mkfile -n 1m small.bin && mkfile -n 5m mid.bin && mkfile -n 200m big.bin && echo hello > note.txt` | files exist |

Record: macOS version ____  build ____  backend ____  Windows build ____

## 1. First run, sign-in, privacy alert (phase 04 open item, ADR 009 #7) MUST

| # | Step | Expected | Result |
|---|---|---|---|
| 1.1 | Launch the app. | Menubar icon appears, no Dock icon; popover shows "Sign in…" | [ ] |
| 1.2 | Sign in with your email, enter the 6-digit code from the mail (arrives within ~1 min). | "This Mac is your first device" (new account) or "Pair this Mac" (existing). Wrong code shows an error, not a crash | [ ] |
| 1.3 | Before copying anything, note System Settings > Privacy & Security > Paste from Other Apps: fsync is absent or "Ask". | state recorded: ____ | [ ] |
| 1.4 | Set Settings > Sync > Clipboard to "Share automatically". Copy some text in Safari. | On macOS 15.4+/26 the system alert "fsync would like to paste from other apps" appears once, OR the onboarding step explains the setting. Record which. | [ ] |
| 1.5 | Choose "Allow" / set "Always allow" in System Settings. Copy text again, quit and relaunch the app, copy again. | No second alert after relaunch (the allowance persists). Record for ad-hoc and for signed build. | [ ] |
| 1.6 | Set it to "Don't allow" / "Deny", copy text. | App does not crash or spin; onboarding/message tells you what to change; manual send (hotkey or menu) still works | [ ] |
| 1.7 | Press Ctrl-Opt-C (global hotkey) with text on the clipboard while fsync is in the background. | Clip is sent. Record whether the alert shows for a hotkey send (ADR 009 open question) | [ ] |

## 2. Mac <-> Windows GUI pairing MUST

| # | Step | Expected | Result |
|---|---|---|---|
| 2.1 | Pair in both directions over two runs. Run A: Windows app shows its pairing offer, on the Mac use popover > "Add device…" and enter the code. Run B (after removing, 2.7): Mac is the offerer (needs-pairing screen "Pair this Mac" or the offer view with "Code" / "QR" modes); enter the code on Windows. Same email on both. | Code (and QR) shown, "Expires in m:ss" counts down, "Get a new code" works after expiry | [ ] |
| 2.2 | Enter the code on the other device. | Both show the same word list ("Check the words"); mismatch must not pair | [ ] |
| 2.3 | Click "They don't match" on one side (repeat run). | Pairing fails with "Pairing didn't finish", no device added | [ ] |
| 2.4 | Click "They match" on both. | Both lists show the other device; status becomes online (LAN or Relay badge) within ~10 s | [ ] |
| 2.5 | Quit the Windows app, wait 30 s. | Mac shows it offline, menubar icon changes | [ ] |
| 2.6 | Start the Windows app again. | Back online without re-pairing (note time; target < 30 s, known slow after restart) | [ ] |
| 2.7 | Mac: remove the Windows device (Your devices > Remove > confirm). | It disappears on the Mac and the Windows app shows removed/trust-reset state. Re-pair works (repeat 2.1-2.4) | [ ] |
| 2.8 | With the Windows PC on another network (phone hotspot), repeat 2.4 and 3.1. | Works via relay | [ ] |

## 3. Clipboard round trip Mac <-> Windows MUST

| # | Step | Expected | Result |
|---|---|---|---|
| 3.1 | Mac: copy "hello" text, paste on Windows (Ctrl-V, Notepad). | arrives within ~2 s, identical | [ ] |
| 3.2 | Windows: copy text, paste on Mac (Cmd-V, TextEdit). | arrives, identical, no copy loop (clipboard does not keep changing; history does not fill with duplicates) | [ ] |
| 3.3 | Copy rich text (TextEdit RTF), then HTML (Safari selection) on Mac, paste into Word/Outlook on Windows. | Formatting kept | [ ] |
| 3.4 | Copy an image (Preview Cmd-C, a screenshot with Ctrl-Cmd-Shift-4) on Mac, paste on Windows (Paint). | Image arrives, correct size | [ ] |
| 3.5 | Windows: copy an image, paste in Preview (File > New from Clipboard). | Image arrives | [ ] |
| 3.6 | Copy 2 MB of text. | Arrives; a clip above "Largest clip" (Settings > Storage) is refused with a message | [ ] |
| 3.7 | Settings > Sync > "Only when I send". Copy on Mac, nothing should reach Windows. Press Ctrl-Opt-C, or menu "Send clipboard". | Only now arrives | [ ] |
| 3.8 | Manual mode, copy on Windows. | Mac menu offers "Paste from <device>"; choosing it puts the clip on the pasteboard | [ ] |
| 3.9 | Switch back to "Share automatically". | Sync resumes without restart | [ ] |

## 4. File paste matrix (ADR 009) MUST for Finder, SHOULD for Mail/Messages

Run from Windows to Mac (copy file in Explorer, paste on Mac). Repeat with the strategy picker in
Settings > Storage > "Files you paste" = "Small files copied now, large ones when pasted" (default), then
"Always when pasted" (lazy) and "Always copied now" (eager). ADR 009: if Finder fails on lazy, set eager as the default.

| # | Step | Expected | Result |
|---|---|---|---|
| 4.1 | Copy `small.bin` (1 MiB) on Windows. Finder: open a folder, Cmd-V. | File appears, byte-identical (`shasum -a 256` both sides) | [ ] |
| 4.2 | Same with `mid.bin` (5 MiB). | Same | [ ] |
| 4.3 | Same with `big.bin` (200 MiB), strategy hybrid (lazy above 20 MiB). Cmd-V in Finder. | Finder waits (progress/spinner), no hang beyond the transfer; file complete, hash matches; menu shows transfer progress and Cancel | [ ] |
| 4.4 | Cancel mid-transfer from the menu (Cancel next to the row), then paste again. | Cancelled cleanly, no partial file left pasted; a new paste works | [ ] |
| 4.5 | Cmd-V of a lazy clip in Mail (new message, attachment area), Messages (compose box). | Attachment added (SHOULD); if it fails with lazy, check eager works | [ ] |
| 4.6 | Cmd-V (or drop target) in Slack, VS Code (Explorer pane), Chrome upload field (paste in a web page that accepts files). | Works, or documented as failing (nice to have) | [ ] |
| 4.7 | Quit fsync while a lazy clip is current, then Cmd-V in Finder. | Nothing is pasted (clip was cleared on quit), no hang, no stale entry | [ ] |
| 4.8 | Copy a file on Mac, paste in Explorer on Windows. | Arrives, hash matches | [ ] |
| 4.9 | On a device row (popover or "Your devices") click "Send files…" and pick a file; then drag a file onto the device row. | Windows shows the offer; accepted file arrives, hash matches | [ ] |
| 4.10 | Windows sends a file to the Mac with Settings > Sync > "Receive files from my devices without asking" off. | Offer row with Accept / Decline under the menu; Decline discards; Accept saves to ~/Downloads | [ ] |
| 4.11 | Turn the setting on; send again. | Lands in ~/Downloads without a prompt | [ ] |
| 4.12 | Check the cache folder `~/Library/Caches/fsync/inbox` after eager pastes. | Exists, mode 0700 (`ls -ld`); old items removed per retention | [ ] |

## 5. Password managers and sensitive data MUST

For each: copy the secret, wait 3 s, then check Windows (and Mac History window).

| # | Step | Expected | Result |
|---|---|---|---|
| 5.1 | 1Password: copy a password via its Copy button or `Cmd-Shift-C`. | Never reaches Windows, not in History (concealed marker honored) | [ ] |
| 5.2 | Bitwarden (desktop or browser extension): copy a password. | Not synced (if it does, note the bundle id; add it to Settings > Privacy denylist and retest) | [ ] |
| 5.3 | Keychain Access: copy a password (Edit > Copy Password to Clipboard). | Not synced | [ ] |
| 5.4 | Safari: autofill / copy from Passwords app or Settings > Passwords. | Not synced | [ ] |
| 5.5 | Settings > Privacy: add a bundle id (e.g. `com.apple.TextEdit`), copy from that app. | Not synced; remove it, copy again, synced | [ ] |
| 5.6 | Using "Send clipboard" from a denylisted app. | Sent (explicit send skips the denylist) but a concealed item is still refused | [ ] |
| 5.7 | Run `log stream --predicate 'subsystem == "app.fsync.mac"' --info` while copying text "SECRET-CANARY-123". | The canary text never appears in the log (types and sizes only) | [ ] |

## 6. History window, settings, lifecycle

| # | Step | Expected | Result |
|---|---|---|---|
| 6.1 | Menu > "History…". Copy several clips from both devices. | Entries listed newest first with device name and time | [ ] |
| 6.2 | Search a word; pin one; delete one; "Load more" with > 1 page. | Each works, pinned shows after relaunch | [ ] |
| 6.3 | History window: click the copy icon ("Copy again") on a long text clip (> 200 chars), a rich/HTML clip and an image clip; paste each in TextEdit/Pages/Preview. | Full text (not the 200-char preview), formatting, image restored; not re-sent to the other device (no echo). File clips show no copy icon | [ ] |
| 6.4 | Clear… > "Clear all but pinned", then "Clear everything". | Unpinned gone / all gone | [ ] |
| 6.5 | Settings > General > "Launch fsync at login", log out and in. | App starts in the menubar; if blocked, the caption "Approve fsync under Login Items in System Settings." shows with an "Open System Settings" button that opens the pane | [ ] |
| 6.6 | Settings > Storage: change retention days, size limit; relaunch. | Values persist | [ ] |
| 6.7 | Lid-close/sleep for 2 min, wake, copy on Windows. | Arrives within ~30 s | [ ] |
| 6.8 | Sign out (Settings > Account > "Sign out…", confirm "Sign out"), then sign in again. | Back to sign-in; device keys survive (no re-pair needed for same account) | [ ] |
| 6.9 | Pending revoke: turn Wi-Fi OFF, sign out, quit fsync. Check `security find-generic-password -s app.fsync.mac -a pending_revoke >/dev/null && echo present` (do not print the item with `-w`). Turn Wi-Fi ON, relaunch fsync, wait 15 s, rerun the check. | `present` after the offline sign-out, absent after the relaunch (retry revoked the token; the revoke on the real backend is also proven by `RealBackendRevokeTests`). After 5 launches that cannot reach the backend, or 30 days, the entry is dropped without a revoke. Never share the refresh token | [ ] |
| 6.10 | Idle CPU: Activity Monitor, fsync for 2 min with auto sync idle. | < 1% CPU | [ ] |

## 7. Accessibility, errors

| # | Step | Expected | Result |
|---|---|---|---|
| 7.1 | VoiceOver (Cmd-F5): navigate the popover and the pairing screen. | All buttons labeled, QR has a label "Pairing QR code" | [ ] |
| 7.2 | Turn Wi-Fi off, try "Send code" at sign-in. | Plain error message, not a stack or silent failure | [ ] |
| 7.3 | Dark mode and light mode. | Text readable in popover, history, settings | [ ] |

## Report back

Send: macOS versions and builds, table of failed row numbers with one line each, the Console excerpt for any
crash (`~/Library/Logs/DiagnosticReports/FsyncMac*`), and the answers to the ADR 009 open questions:
1. Does the hotkey send trigger the privacy alert (1.7)? 2. Does "Always allow" persist across relaunch for ad-hoc vs signed build (1.5)? 3. Finder / Mail / Messages / Slack / VS Code / Chrome lazy vs eager results (section 4).
4. Do real 1Password / Keychain / Safari copies arrive as concealed (section 5)?

## Notes
- Round-2 C3 is implemented (full-content copy-back via `history_item_reps`); row 6.3 covers it.
- The deployed backend returned 502 on 2026-10-15 03:10 UTC+7 during this round (reason unknown, likely a deploy by another track); confirm `curl https://api-fsync.10z.one/health` says `ok` before starting.
