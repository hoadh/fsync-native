# Desktop UI redesign: shared spec (macOS + Windows)

Plan: `plans/261019-0016-desktop-ui-redesign-macos-windows/`. Both tracks implement this. Deviations go in phase reports; phase 12 reconciles. Windows names below are UIA Names read by `apps/windows/tests/e2e/*` (lookup is `FindFirst(Descendants)` by exact Name, so a duplicate Name anywhere on screen is a latent e2e break).

## 1. Copy table
e2e-locked = Y: exact string, must not change, must not be duplicated as another element's Name on the same screen. `{x}` = placeholder.

### 1a. Locked by e2e (Windows)
| Key | macOS | Windows | e2e-locked |
|---|---|---|---|
| login.email.label | Email | Email | Y |
| login.sendCode | Send code | Send code | Y |
| login.code.label | Code | Code | Y |
| login.signIn | Sign in | Sign in | Y |
| pair.offerCode | Show a code | Show a code | Y |
| pair.code | 6 digits (`^\d{6}$`, own element, no label text in same element) | same | Y |
| pair.sas | SAS words, one element, verbatim | same | Y |
| pair.sasMatch | The words match | The words match | Y |
| pair.done.title | Paired | Paired (InfoBar title) | Y |
| nav.devices | Devices | Devices | Y |
| nav.history | History | History | Y |
| nav.files | Files | Files | Y |
| nav.settings | Settings | Settings (footer item; forced Name `Settings`) | Y |
| device.row | `{Name}, {Platform}, {Status}` | `DeviceItem.ToString()` = `{Name}, {Platform}, {Status}` | Y |
| device.name | `{Name}` (own element; e2e finds `devA`) | same | Y |
| device.status.online | contains `Online` | `Online, not connected yet` (pattern `*devA*Online*`) | Y |
| device.status.offline | contains `Offline` | `Offline` (pattern `*devA*Offline*`) | Y |
| device.remove | Remove | Remove | Y |
| device.remove.dialog.title | Remove {name}? | Remove {name}? | Y |
| device.remove.dialog.confirm | Remove | Remove | Y |
| history.copy | Copy | Copy | Y |
| files.accept | Save to Downloads | Save to Downloads | Y |
| files.sendTo.button | `{device name}` | `{device name}` (button Name = device name) | Y |
| settings.startup | Launch at login (macOS) | Start fsync when I sign in to Windows | Y |
| settings.manual | Send my clipboard only when I ask | Send my clipboard only when I ask (manual mode) | Y |
| settings.eagerFiles | Download copied files before pasting | Download copied files before pasting (compatibility) | Y |
| settings.save | (none: saves on change) | Save | Y |
| settings.saved | (none) | Saved. | Y |

### 1b. Page titles (none equals a nav name)
| Key | macOS | Windows | e2e-locked |
|---|---|---|---|
| title.devices | Your devices | Your devices | N |
| title.history | Clipboard history | Clipboard history | N |
| title.files | Files and transfers | Files and transfers | N |
| title.pairing | Pair this Mac | Pair this PC | N |
| title.settings | Settings (system window title) | Preferences | N |
| nav.pairing | Pair a device | Pair a device (unchanged) | N |

### 1c. Other copy (non-locked, from current code)
| Key | macOS | Windows | e2e-locked |
|---|---|---|---|
| login.title | Sign in to fsync | Sign in to fsync | N |
| login.useOther | Use another email | Use another email | N |
| pair.join.cta | Pair | Pair | N |
| pair.sas.reject | They don't match | They don't match | N |
| pair.failed.title | Pairing failed | Pairing failed | N |
| devices.hint | Drop files on a device to send them. | same | N |
| devices.empty | No other devices yet. Pair one to start syncing. | same | N |
| history.empty | Nothing copied yet. | same | N |
| files.empty | No transfers yet. Drop files on a device in Devices. | Drop files on a device in Devices, or use Send to > fsync in Explorer. | N |
| files.decline | Decline | Decline | N |
| files.offerSection | Waiting for you | Waiting for you | N |
| trust.title | Your trusted devices were reset | same | N |
| trust.body | Someone signed in to your account and reset the trusted devices. Only accept if you did this yourself or know who did. Check that the devices below are the ones you expect. If you keep the current devices, this {Mac/PC} continues to trust the devices it trusts now. | same, "this PC" | N |
| trust.list | New device list ({n}) | same | N |
| trust.keep (default) | Keep current devices | same | N |
| trust.trust (destructive) | Trust new devices | same | N |
| trust.confirm.title | Trust the new devices? | same | N |
| trust.confirm.body | This replaces the devices this {Mac/PC} trusts with these {1 device / n devices}:\n{names}\n\nOnly continue if you or someone you trust did the reset. | same | N |
| trust.confirm.cancel | Cancel | Cancel | N |
| trust.notice.changed | The device list changed. Review it again. | same | N |
| trust.err.notFound | This request changed or was already answered. Review the latest one. | same | N |
| trust.err.network | Couldn't reach the server. Check your connection and try again. | same | N |
| trust.err.auth | Please sign in again, then retry. | same | N |
| trust.err.other | Something went wrong. Please try again. | same | N |
| trust.out.trusted | New devices trusted / This {Mac/PC} now trusts the new device list. | same | N |
| trust.out.needsPairing | Pair this {Mac/PC} again / This {Mac/PC} is not in the new list; pair it again. | same | N |
| trust.out.kept | Current devices kept / This {Mac/PC} keeps trusting its current devices. | same | N |
| trust.out.done | Done | Done | N |
| recovery.link | Lost your other devices? | same | N |
| recovery.1.title | Lost your other devices? | same | N |
| recovery.1.body | If you can't pair with another device, this {Mac/PC} can become the only trusted device on your account. Every other device you own will have to confirm the change or pair again before it syncs with you. | same | N |
| recovery.1.continue | Continue… | Continue... | N |
| recovery.2.title | Reset trust for your account? | same | N |
| recovery.2.body | Your other devices will stop syncing until you confirm the reset on them or pair them again. This can't be undone, and it can be done only a few times a day. | same | N |
| recovery.2.destructive | Reset and trust only this {Mac/PC} | same | N |
| recovery.cancel (default) | Cancel | Cancel | N |
| notif.title | Files from {sender} | same | N |
| notif.body.auto | {n} files, {size}. Saving to Downloads | same | N |
| notif.body.ask | {n} files, {size}. Open fsync to accept | same | N |
| notif.body.coalesced | {k} offers from {sender} | same | N |

## 2. AutomationId list (Windows)
`NavDevices`, `NavHistory`, `NavFiles`, `NavPairing`, `NavSettings`, `StartingRing`, `BootFailedInfoBar`, `ResetHistoryButton`, `TrustTrustButton`, `TrustKeepButton`, `RecoveryLink`. Adding an AutomationId never changes the UIA Name that e2e reads; set `AutomationProperties.Name` explicitly where a nav item needs a forced Name.

## 3. Icon map
| Concept | SF Symbol (macOS 14) | Segoe Fluent glyph (Segoe MDL2 fallback) |
|---|---|---|
| Devices | `laptopcomputer.and.iphone` | E772 Devices |
| History | `clock.arrow.circlepath` | E81C History |
| Transfers / Files | `arrow.up.arrow.down.circle` | E8AB Switch (up/down) |
| Pairing | `link.badge.plus` | E8FA AddFriend |
| Settings | `gearshape` | E713 Setting |
| Status: good (LAN/Relay) | `circle.fill` green | filled ellipse, SystemFillColorSuccess |
| Status: online, not connected | `circle.fill` orange | filled ellipse, SystemFillColorCaution |
| Status: offline | `circle` secondary | filled ellipse, SystemFillColorNeutral |
| Offline / no network | `wifi.slash` | E783 Disconnect (present in MDL2 and Fluent) |
| Warning | `exclamationmark.triangle.fill` orange | E7BA Warning |

Stretch (phase 06/11): menubar icon-state glyphs.

## 4. Notification rules
Incoming files only. No sound. Never file names, clipboard content or raw node ids.
| Rule | Spec |
|---|---|
| Title | `Files from {sender}`, sender via sanitizer (section 5) |
| Body | `{n} files, {size}` then `Saving to Downloads` (auto-accepted) or `Open fsync to accept` |
| Coalescing | max one notice per sender per 10 s; later offers in the window update the count: `{k} offers from {sender}` |
| Activation | arrival never activates the app or opens/focuses a window; only an explicit click does |
| Click | opens Transfers (macOS) / Files (Windows) page |
| Suppression | suppressed when that page is visible and the app is active |
| Windows toast args | fixed constant set (`open-files`); anything else ignored. Cold start with toast args only opens the window |

## 5. Sanitizer spec (peer-supplied text)
Reference: `PeerText.sanitize(_:limit: 40)` in `apps/macos/FsyncMac/Sources/FsyncMacCore/Features/Devices/PeerText.swift`. Algorithm, in order:
1. Remove every Unicode scalar with general category Cc (control: newline, tab, NUL, ...) or Cf (format: bidi U+202A-202E, U+2066-2069, ZWJ/ZWNJ U+200D/U+200C, U+200B, U+FEFF). Removed, not replaced by a space.
2. Trim leading and trailing whitespace (Swift `.whitespaces`: space, tab, Zs separators). Interior whitespace is kept.
3. If length > 40, keep the first 40 and append `…` (U+2026, one char). Result is 41 chars. Length counts grapheme clusters (Swift `Character`); Windows must count text elements (`StringInfo`), not UTF-16 units. Cut is after trim, so a space at position 40 stays before the ellipsis.
4. `sanitize` itself returns `""` for empty. Callers show `another device` when the result is empty (`PeerText.deviceName` does; Windows helper must do the same).

Test vectors (input with escapes -> expected output). Both platforms copy these.
| # | Input | Output |
|---|---|---|
| 1 | `"Alice\nBob"` | `"AliceBob"` |
| 2 | `"\u{202E}evil"` | `"evil"` |
| 3 | `"gp\u{202E}j.exe"` | `"gpj.exe"` |
| 4 | `"  Mac  "` | `"Mac"` |
| 5 | `"a\u{200D}b\u{200B}c"` | `"abc"` |
| 6 | `"a" x 40` | same 40 `a` (no ellipsis) |
| 7 | `"a" x 41` | `"a" x 40 + "…"` |
| 8 | `"a" x 39 + " " + "b"` (41 chars) | `"a" x 39 + " " + "…"` |
| 9 | `"\u{0007}\u{200B}\n"` (only controls/format) | `""` (caller: `another device`) |
| 10 | `"a  b"` | `"a  b"` |
| 11 | `"👍🏽" x 41` (41 clusters, 2 scalars each) | `"👍🏽" x 40 + "…"` |

## 6. State matrix
Cell = what the screen shows. Windows has an Offline app phase (InfoBar + Retry, 15 s auto-retry). macOS has no Offline phase: Offline is an inline banner only.
| Screen | Loading | Empty | Error | Offline | Populated |
|---|---|---|---|---|---|
| Devices | skeleton rows / ProgressRing | `devices.empty` + Pair CTA | error InfoBar/banner, list kept | Win: warning InfoBar + Retry; mac: banner | rows with status dot |
| History | ProgressRing on first load | `history.empty` | error InfoBar/banner | list stays (local); banner only | rows + Load more |
| Files / Transfers | ProgressRing | `files.empty` | error InfoBar/banner | Send disabled, banner | offers, transfers (Cancel/Retry/Show), Clear finished |
| Pairing | Joining/Waiting progress | stage Idle: Show a code / join box | `Pairing failed` InfoBar with message | Win: gated; mac: banner | Offering code/QR, SAS, `Paired` |
| Settings | n/a | n/a | error InfoBar/banner | editable (local) | form; Win Save + `Saved.`, mac saves on change |
| Shell (boot) | `StartingRing` | n/a | `BootFailedInfoBar` + Retry | see Devices | nav + page |

## 7. Destructive dialog rule
- Default button (Enter/Return) = Cancel or Close. The destructive button is never default and never the Enter target.
- A dialog is bound to the id it opened with (`resetId`, device id). It closes if the displayed id changes, and the answer uses the captured id, not the current one (`TrustResetMapping.dialogIsPresented`).
- Applies to: Remove device, Clear history, Start a new history, Trust new devices, Reset trust (recovery), Sign out.
- Windows: `ContentDialog.DefaultButton = Close`; one dialog at a time (existing `Dialogs.cs` guard).
