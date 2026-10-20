# Desktop UI Redesign QA Checklist

Last updated: 2026-10-20. Scope: this redesign only (plan `plans/261019-0016-desktop-ui-redesign-macos-windows`, MVP rows only). Older plans' manual QA (for example MVP phases 04, 12 and 14) is not re-tested here.

## Before you start

| Need | Detail |
|------|--------|
| Build | `apps/macos/scripts/build-fsync-mac-app.sh --api <URL> --relay <URL>`, then open `apps/macos/build/FsyncMac.app`. A bundle is required for notifications and Launch at login |
| Host | macOS 26 only. macOS 14 is untested (accepted risk, see the last row) |
| Accounts | Two devices on one account: this Mac and one peer (another Mac, or `fsync-cli` on the same account) |
| Permissions | Allow notifications for fsync when prompted |

Record each result as `PASS`, `FAIL` (add the evidence: screenshot path or a one-line note) or `N/A` (add the reason). Any `FAIL` in an MVP row blocks the release.

## macOS

| Item | Steps | Expected | Result |
|------|-------|----------|--------|
| Sidebar shell | Sign in and wait for the main window | Sidebar lists Devices, History, Transfers. Devices is selected | |
| Transfers badge | Have the peer send a file; look at the sidebar | Transfers row shows a badge with the number of waiting offers | |
| Devices page | Open Devices | Header "Your devices" with a count. Each row shows a status dot and status text (LAN, Relay, Online or Offline). Offline rows show "Last seen …" | |
| Devices empty state | Use an account with only this Mac | "No other devices yet" with a "Pair device…" button | |
| Drop to send | Drag one or more files onto a device row | Sheet "Send N items to {device}?" lists each file with its size. Cancel is the default (Return cancels). Send starts the transfer | |
| Remove device | Device row "Remove…" | Dialog "Remove {name}?". Cancel is the default; Remove only removes after confirmation | |
| History page | Open History; search; Clear… menu | Title "Clipboard history". Search filters rows. "Clear…" asks for confirmation with Cancel as default. Empty state "Nothing copied yet." | |
| Transfers page | Open Transfers with and without transfers | Table columns Name, Direction, Size, Progress, Status. Empty state "No transfers yet". "Clear finished" is disabled when nothing has finished | |
| Retry | Make an incoming transfer fail (for example, stop the peer mid-transfer), with the peer still trusted | Row action "Retry" is enabled. Click starts a new transfer and the failed row is removed. Retry is disabled while it runs | |
| Retry blocked | Same failed transfer, then remove that peer from the device list | "Retry" is disabled | |
| Show in Finder | Open a finished received file's row action "Show in Finder" | Finder opens with the file selected | |
| Pairing sheet | Click "Pair device…" in the sidebar or toolbar; press Esc | Sheet opens with the code and QR. Esc or Cancel closes it. Done appears after a successful pairing | |
| Settings window (no window open) | Close all fsync windows; popover "Settings…" | Settings window opens in front and gets focus. Repeat 5 times, including after closing and reopening | |
| Settings window (main open) | Main window open; press ⌘, and also popover "Settings…" | Settings comes to the front in both cases | |
| Settings tabs | Open each tab: General, Sync, Privacy, Storage, Account | Changes save on change (no Save button). Account "Sign out" asks "Sign out of fsync?" with Cancel as default | |
| Popover | Click the menubar icon | Width about 280 pt. Sections for status, clipboard, files and devices. Entries "History…", "Transfers…" and "Pair device…" open the main window on the matching page | |
| Hidden helper | Open Window menu and press ⌘` to cycle windows | The Settings opener helper is not listed and is never focused. No Dock icon appears | |
| Notification: single offer | Peer sends one file from a Mac | One notification "Files from {sender}" with body "1 file, {size}. Open fsync to accept" (or "Saving to Downloads" when auto-accepted) | |
| Notification: coalescing | Peer sends 3 offers within 10 seconds | One notification (updated), body "3 offers from {sender}". No second notification for the same sender in that window | |
| No file names | Send files with distinctive names; check the notification banner and Notification Center | Neither the banner nor Notification Center shows any file name or path | |
| No activation on arrival | Put another app in front. Peer sends a file | Notification appears. The frontmost app stays frontmost, and no fsync window is raised or focused | |
| Click opens Transfers | Click the notification while fsync runs | Main window comes forward on Transfers | |
| Cold-launch click | Quit fsync (menu "Quit fsync"). Click the notification still in Notification Center | fsync launches and the main window opens on Transfers | |
| Suppression | Transfers visible and fsync active; peer sends a file | No banner appears | |
| Trust gate | Reset trusted devices from another device | Trust prompt lists the new device names without control characters. "Keep current devices" is the default. "Trust new devices" asks for confirmation | |
| macOS 14 | Not tested | Not verified. Risk accepted by the owner (plan validation session 1). Release note states the tested version is macOS 26 | |

## Windows

TODO: add the Windows rows after phases 09-10 finish (plan phase 12, Windows half). Do not fill this section before then.
