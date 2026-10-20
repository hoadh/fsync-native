using Fsync.App.Views;

namespace Fsync.App.Shell;

/// <summary>Navigation tags (nav item Tag, tray shortcuts, ShowWindow) and the page each one opens.</summary>
internal static class NavigationRoutes
{
    public const string DevicesTag = "devices", HistoryTag = "history", FilesTag = "files", PairingTag = "pairing", SettingsTag = "settings";

    /// <summary>The page for a tag; unknown tags open Devices.</summary>
    public static Type PageFor(string? tag) => tag switch
    {
        HistoryTag => typeof(HistoryPage),
        FilesTag => typeof(TransfersPage),
        PairingTag => typeof(PairingPage),
        SettingsTag => typeof(SettingsPage),
        _ => typeof(DevicesPage),
    };

    /// <summary>The tag of a routed page, or null for pages outside the nav (onboarding, gates).</summary>
    public static string? TagFor(Type? page) =>
        page == typeof(DevicesPage) ? DevicesTag
        : page == typeof(HistoryPage) ? HistoryTag
        : page == typeof(TransfersPage) ? FilesTag
        : page == typeof(PairingPage) ? PairingTag
        : page == typeof(SettingsPage) ? SettingsTag
        : null;
}
