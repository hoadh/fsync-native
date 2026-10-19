# UI inventory: macOS (SwiftUI) + Windows (WinUI 3), 2026-10-19

## macOS `apps/macos/FsyncMac`
- `Package.swift` `.macOS(.v13)`, tools 6.0. Targets: `FsyncMacCore` (logic, no SwiftUI), `FsyncMacUI`, `FsyncMac` (`@main`).
- Scenes (`Sources/FsyncMac/FsyncMacApp.swift`): `MenuBarExtra(.window)` -> `MenuPopover` (icon `doc.on.clipboard(.fill)` filled when peer online; auto-opens main once in signedOut/needsPairing/bootFailed); `Window("fsync", id:"main")` -> `RootView` (AppDelegate closes at launch); `Window("Clipboard history", id:"history")` -> `HistoryView`; `Settings` -> `SettingsView` (via `showSettingsWindow:`).
- AppKit: `.accessory` policy, `NSApp.activate`, `MenuWindowTopAnchor` (NSViewRepresentable keeps popover top edge), `FilePicker` (NSOpenPanel), `PairingPasteboard`.

| View (Sources/FsyncMacUI) | Purpose | Model | Host |
|---|---|---|---|
| MenuPopover | phase switch; ready -> MenuBarContentView; else status + Sign in/Set up/Settings/Quit; width 280 | EngineBridge | popover |
| Menu/MenuBarContentView | status, banner, trust link, clipboard + files sections, devices, actions | DevicesModel, TrustResetPromptModel, ClipboardSync?, FilesSync?, TransfersModel? | popover |
| Menu/StatusHeaderView, ClipboardMenuSection, FilesMenuSection | status; paste from X / send clipboard / History…; offers Accept/Decline + active transfers Cancel | StatusSummary, ClipboardSync, FilesSync, TransfersModel | popover |
| RootView (+BootingView, BootFailedView) | screen per AppPhase, TrustGate, min 380x300, Start fresh confirm | EngineBridge | main |
| TrustGate | TrustResetPromptView while pending | TrustResetPromptModel | main |
| ReadyView | "Your devices" + Remove, Add device…, Sign out; swaps PairingFlowView(.join) | DevicesModel, PairingModel | main |
| SignOutButton | shared confirm | - | popover/main/settings |
| Onboarding/LoginView(+LoginFlowView), OtpView | email -> 6-digit code auto-submit, resend countdown | LoginModel | main |
| Onboarding/NeedsPairingView(+RecoveryLink), RecoverySheet, FirstDeviceView | offer code, "Lost your other devices?" sheet (420w), first device | PairingModel, AccountRecoveryModel | main |
| Pairing/PairingFlowView, PairOfferView, PairJoinView, SasConfirmView, QrCodeImage | states idle/offering/joining/sas/waiting/done/failed; Code/QR segmented, countdown; SAS words; QR 200pt | PairingModel, PairingPeer | main (not sheet) |
| Devices/DeviceListView, DeviceRowView, FilePicker | icon, name, status dot LAN/Relay/Online/Offline, last seen, Send files… + onDrop, Remove | Device, PeerText | popover (no remove), main (no send) |
| History/HistoryView, HistoryRowView | search 250ms debounce, Clear… (keep pinned/all), copy/pin/delete, Load more; min 460x360 | HistoryModel, DevicesModel, ClipboardSync? | history window |
| Settings/SettingsView (+General, Account), SyncSettingsView, PrivacySettingsView, StorageSettingsView | TabView 420x300: General (launch at login), Sync (radio auto/manual, auto-accept), Privacy (denylist list add/remove), Storage (steppers, file-clip strategy), Account (node prefix, sign out). Saves on change | EngineBridge, LaunchAtLogin, ClipSettingsModel | Settings |

- TrustResetPrompt (`apps/macos/Packages/FsyncCoreKit/Sources/TrustResetPrompt/`): PromptView/Content, PendingView ("Trust new devices" destructive confirm / "Keep current devices"), OutcomeView.
- Tests `Tests/FsyncMacUITests/OnboardingRenderTests.swift`: ImageRenderer scale 2, only asserts size > 100x50; skips LoginView/OtpView (@FocusState). `MenuWindowTopAnchorTests.swift`: real borderless NSWindow.

## Windows `apps/windows/src/Fsync.App`
- net8.0-windows10.0.19041, WASDK 1.6.250205002, unpackaged self-contained, x64+ARM64, H.NotifyIcon.WinUI 2.1.4.
- App.xaml merges only XamlControlsResources; built-in text styles; no Mica/SystemBackdrop.
- MainWindow: AppWindow.Resize 720x680, not maximizable, created on demand, destroyed on close. Top NavigationView (tags devices/history/files/pairing/settings, built-in settings hidden), Frame padding 24,16,24,24. Overlay grid (ProgressRing, "Start a new history on this PC" + ContentDialog). Nav gating: SignedOut -> Onboarding; NeedsPairing -> Pairing only; Ready/Offline -> all.
- TrayHost: TaskbarIcon, right-click MenuFlyout (ContextMenuMode.SecondWindow), left click opens window; GeneratedIconSource letter "f" badge colored by StatusKind; items: activity (disabled), Send clipboard now, Paste from X, Clipboard history, Files and transfers, Open fsync, Pair a device..., Settings, Quit fsync.
- Helpers: Views/Ui.cs (x:Bind Show/ShowIf/When/StatusBrush/Expires), Views/Dialogs.cs (single ContentDialog guard).

| Page | Purpose | VM |
|---|---|---|
| OnboardingPage | email -> code, ProgressBar, InfoBar | OnboardingViewModel (per page) |
| DevicesPage | ListView, drop files to send, Remove (ContentDialog), Offline retry InfoBar | DevicesViewModel (singleton), EngineBridge |
| HistoryPage | search, Clear history… (keep pinned), Copy/Pin/Delete, Load more | HistoryViewModel (per page) |
| TransfersPage ("Files") | Send-to device picker, offers Save to Downloads/Decline, transfers Cancel/Retry/Show, Clear finished | TransfersViewModel |
| PairingPage | Show a code / Show a QR code; join code + Pair; SAS; Completed/Failed InfoBar | PairingViewModel (singleton) |
| SettingsPage | one scroll page: General, Privacy (denylist textbox), Storage (NumberBox), explicit **Save**, Account | SettingsViewModel, ClipSettingsViewModel |

- Unit tests `apps/windows/tests/Fsync.App.Tests`: ViewModelTests, Round1Tests, RealEngineViewModelTests, MappingAndStoreTests (TrayStatusViewModel.Compute), FilesTests, CancelStartedTransferTests, HistoryCopyBackTests. No SettingsViewModel test.

### E2E UIA name contract (CRITICAL; scripts `apps/windows/tests/e2e/pair-with-cli.ps1`, `clipboard-with-cli.ps1`, `E2eHost.psm1`, `Uia.psm1`)
Lookup by UIA **Name** only (no AutomationIds set). TextBox Header = Name. Renaming breaks e2e:
- Onboarding: `Email`, `Send code`, `Code`, `Sign in`
- Pairing: `Show a code`, `The words match`, `Paired` (InfoBar title); code element name `^\d{6}$`; SAS words verbatim as one element name
- Nav items: `Devices`, `History`, `Files`, `Settings`
- Devices: device name as element name; row combined name contains `Online`/`Offline` (`*devA*Online*`); `Remove`, dialog title `Remove devA?`, `Remove`
- History: `Copy`
- Files: `Save to Downloads`; Send-to picker buttons named after device
- Settings toggles: `Start fsync when I sign in to Windows`, `Send my clipboard only when I ask (manual mode)`, `Download copied files before pasting (compatibility)`; `Save`, `Saved.`

## Parity gaps
| Feature | macOS | Windows |
|---|---|---|
| Trust-reset prompt | TrustGate + popover link | Missing (FFI has PendingTrustResets/ConfirmTrustReset) |
| Account recovery | RecoverySheet | Missing |
| First-device screen | FirstDeviceView | Missing |
| Transfers page (history, Retry, Show, Clear finished) | only active + Cancel in popover | full Files page |
| Explorer/Finder Send to | none | yes |
| Send files to device | picker + drop on popover row | drop on Devices page |
| History | separate window | tab |
| Pairing | inside main (ReadyView swap), recovery sheet | own tab |
| Pairing QR | segmented + live countdown | two buttons + static "Valid until" |
| Offline state | none | Offline phase, Retry InfoBar, 15s auto-retry |
| Tray/popover | rich popover | MenuFlyout only |
| Settings | tabs, save on change, 3-option file strategy | one page + Save |
| Privacy denylist | list add/remove | free text |

## Docs
- `docs/code-standards.md`: source files < 200 lines (tests may exceed), split by concern; Swift/C# PascalCase; docs/scripts kebab-case.
- `docs/codebase-summary.md`: FsyncMac targets/folders, "Running the macOS app"; Windows -> README.
