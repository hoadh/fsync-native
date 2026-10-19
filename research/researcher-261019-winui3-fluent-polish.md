# WinUI 3 Fluent polish for unpackaged tray app (WASDK 1.6)

Scope: apps/windows/src/Fsync.App. Pin: Microsoft.WindowsAppSDK 1.6.250205002, H.NotifyIcon.WinUI 2.1.4, unpackaged self-contained, Win10 19041+ and Win11.
Budget: 5 searches/fetches. Limits in section 7.

## Verified facts

- Title bar control: `TitleBar` (new control) is added in WASDK 1.7, not 1.6. Source: [Microsoft title bar doc](https://learn.microsoft.com/en-us/windows/apps/develop/title-bar). On 1.6, use `AppWindow.TitleBar` + `ExtendsContentIntoTitleBar` (available since 1.4 per same doc).
- Custom title bar content is required to extend Mica into the title bar (same doc). Title bar color customization is ignored on Win10 (same doc).
- Left vs Top nav: Microsoft suggests Left for 5-10 equal top-level categories. `PaneDisplayMode` = Auto/Left/Top/LeftCompact/LeftMinimal. No Bottom mode (unofficial workaround only). Source: [NavigationView docs](https://learn.microsoft.com/lb-lu/windows/apps/develop/ui/controls/navigationview) (search snippet).
- Settings card package for WinUI 3: `CommunityToolkit.WinUI.Controls.SettingsControls` (contains SettingsCard and SettingsExpander). Latest seen: 8.2.251219. `CommunityToolkit.Uwp.Controls.SettingsControls` is the UWP/Uno package, wrong for us. Source: [NuGet](https://nuget.org/packages/CommunityToolkit.WinUI.Controls.SettingsControls). WASDK 1.6 compatibility NOT confirmed.
- Toasts: Microsoft guidance is `AppNotificationManager` (WinUI 3 and unpackaged Win32 both). Works without package identity. `ToastNotificationManager` is legacy UWP. Sources: [notifications guidance](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/) (search snippets), [UWP-to-WinUI migration guide](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/migrate-to-windows-app-sdk/guides/toast-notifications). Manifest activator step in that guide is packaged-only.
- Title bar control criticism: a [Zenn rich title bar article](https://zenn.dev/shinta0806/articles/rich-title-bar) says the 1.7 TitleBar puts title/subtitle low, with no alignment property. Single source, anecdotal.

## 1. Fluent nav and shell

| Option | Fit for fsync (4 sections + settings) | Cost | Verdict |
|---|---|---|---|
| Top nav, Settings in top-right gear | Fits: 4 sections, Microsoft threshold for Left is 5+ | Low, already in place | Keep |
| Left nav, footer Settings | Overkill for 4 items; eats width the history list needs | Medium | Skip unless sections grow to 5+ |
| Custom bottom nav (style hack) | Unofficial, breaks on WinUI updates | High | Reject |

Concrete fix: `Settings` is currently a MenuItem with `IsSettingsVisible="False"`. Remove it from `MenuItems`, set `IsSettingsVisible="True"`, handle `args.IsSettingsInvoked` in `OnNavSelectionChanged` to route to Settings. This is the standard settings slot and frees a nav item.

Title bar: move to custom title bar (`ExtendsContentIntoTitleBar = true`, AppTitleBar grid with drag region, height 48 per doc). Lets nav sit in the title bar and enables Mica. Needs interactive-content drag region care (nav and caption buttons). Do NOT upgrade WASDK only for `TitleBar` control.

Backdrop: `SystemBackdrop = new MicaBackdrop()` (Win11 only). Guard with `MicaController.IsSupported()` and fall back to solid `ApplicationPageBackgroundThemeBrush`. Mica does not render on Win10; do not rely on it for layout.

## 2. Page patterns

- Status/errors: replace plain `OverlayText` TextBlock with `InfoBar` (Severity Error/Warning, closable false, title + message). Keep `ProgressRing` for starting state. Standard Fluent for failed boot.
- Empty states: one centered `TextBlock` (BodyStrong title + Caption subtitle) + one action button. Use same stack pattern as `Overlay`.
- Settings page: `SettingsCard` / `SettingsExpander` only if the toolkit builds on 1.6. If it does not, use plain `StackPanel` rows with `TextBlock` labels. YAGNI: skip the package unless Settings grows.
- ItemsView/ListView: use `ItemsRepeater` or `ListView` with item template and `DataTemplate` + `ItemsView` (WinUI 2.8+). Use `Grid` with `ColumnDefinitions` for device name / time / actions.
- Onboarding: `TeachingTip` anchored to the Pair button on first run, one-time (store flag in settings). Non-blocking.
- Confirmations: `ContentDialog` for destructive actions (Reset history, Quit with pending transfers). Set `XamlRoot` to the window content root (required in unpackaged/Window hosts).
- Drag and drop files: `AllowDrop="True"` on Files page, handle `DragOver` (set `AcceptedOperation = Copy`) and `Drop`. Show drop hint via `InfoBar` or dashed border.

## 3. Tray UX

- Keep `MenuFlyout` as `ContextFlyout` (already done). Windows 11 style custom tray popups (Phone Link / OneDrive-like) need a custom flyout window; not verified in H.NotifyIcon 2.1.4 docs. Cost high, value medium. Defer (YAGNI).
- `ForceCreate(enablesEfficiencyMode: false)`: keep false. A clipboard sync tray must stay responsive; efficiency mode throttles it.
- Toasts: use `AppNotificationManager.Default`. For unpackaged apps, register on startup (`AppNotificationManager.Default.Register()`) and `Unregister()` on exit. Also needs `AppNotificationManager.Default.Register` before `Show`. Verify in a test build (not confirmed by source here). Toast on "paste pending" or "transfer received" is the highest-value notification.
- Badge: current hard-coded colors (green/orange/red/grey) are acceptable for a tray icon. Keep glyph "f" with high contrast (white on dark). Test on light taskbar.

## 4. Accessibility and UIA e2e

- Keep `AutomationProperties.AutomationId` stable on every control the e2e tests touch (Nav items, InfoBar, ResetHistoryButton, Overlay). IDs are the e2e contract; do not rename casually.
- Set `AutomationProperties.Name` on icon-only buttons and the `ProgressRing` (e.g., "Starting fsync").
- Access keys: `UIElement.AccessKey` on nav items (single letter, e.g., "H" History). Use `KeyboardAccelerator` for global actions (already exists for clipboard hot keys).
- High contrast: use `{ThemeResource}` brushes everywhere. Hard-coded colors only in the tray badge.
- Narrator/UIA tree check not done here (see limits).

## 5. Theme and typography

- Type ramp (WinUI built-in): `TitleTextBlockStyle` (page title), `SubtitleTextBlockStyle` (section), `BodyTextBlockStyle`, `BodyStrongTextBlockStyle`, `CaptionTextBlockStyle` (meta). Use these instead of ad-hoc `FontSize`.
- Spacing: `Frame Padding="24,16,24,24"` already on 8px grid. Keep 4/8/12/16/24 steps. Use `StackPanel Spacing` not margins.
- Light/dark: rely on `RequestedTheme` default (follows system). Accent: `{ThemeResource SystemAccentColorLight2Brush}` etc. Do not hard-code brand colors in pages.

## 6. Prioritized recommendations (impact / cost)

1. Settings to footer (`IsSettingsVisible=True`, `IsSettingsInvoked`). High / Low.
2. Overlay failure states to `InfoBar` + named `ProgressRing` + stable AutomationIds. High / Low.
3. Type ramp + spacing via built-in styles. Medium / Low.
4. Mica backdrop with `MicaController.IsSupported()` fallback. Medium / Low.
5. Toasts via `AppNotificationManager` with unpackaged Register/Unregister. High / Medium. Verify first.
6. Custom title bar (`ExtendsContentIntoTitleBar`, drag region, Mica into title). Medium / Medium.
7. Drag-and-drop on Files page + `ContentDialog` confirms + `TeachingTip` onboarding. Medium / Medium.
8. SettingsCard via toolkit. Low-Medium / Low, only if 1.6 build passes. Else skip.
9. Defer: WASDK 1.7 upgrade (for `TitleBar`), custom tray flyout, left nav. Low / High.

## 7. Trade-off summary

| Dimension | Option A (recommended) | Option B |
|---|---|---|
| Complexity | Built-in controls + Mica guard | Custom title bar + toolkit |
| Maintenance | No new packages | +1 NuGet (toolkit), version drift with WASDK |
| Adoption risk | WinUI built-ins: low | Toolkit: medium, 1.6 compat unverified |
| Win10 | Solid fallback | Mica none; title bar color ignored |

## Limitations

- 5 search/fetch budget: only the title bar doc was read in full. Nav, toolkit, toast, and typography claims rest on search snippets (single source each for some). Confidence: medium.
- Not checked: WASDK 1.8 release notes, toolkit compat with 1.6, H.NotifyIcon 2.1.4 flyout API, unpackaged toast registration steps, Narrator behavior, high-contrast rendering.
- No build or run. All XAML/C# snippets are unverified against this project.

## Unresolved questions

1. Does CommunityToolkit.WinUI.Controls.SettingsControls build against WASDK 1.6.250205002? Test with a compile.
2. Is `AppNotificationManager.Default.Register()` required for unpackaged toasts, and does the self-contained build need extra steps?
3. Does H.NotifyIcon 2.1.4 expose a custom popup/flyout API for tray?
4. Should the team upgrade to WASDK 1.7+ for `TitleBar`, and what is the Win10 support floor there?
5. Mica fallback on Win10: is `DesktopAcrylicBackdrop` acceptable or is solid preferred?
