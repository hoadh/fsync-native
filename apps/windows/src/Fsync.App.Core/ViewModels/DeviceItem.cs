using Fsync.Core;

namespace Fsync.App.ViewModels;

public enum StatusKind { Neutral, Good, Warning, Bad }

/// <summary>One row of the device list, already in display terms.</summary>
public sealed record DeviceItem(
    string NodeId, string Name, string PlatformText, string StatusText, StatusKind Kind,
    string LastSeenText, bool IsSelf, bool CanRemove)
{
    public string ShortId => NodeId.Length > 8 ? NodeId[..8] : NodeId;

    /// <summary>ListView uses this as the row's accessible name (screen readers, UI Automation).</summary>
    public override string ToString() => $"{Name}, {PlatformText}, {StatusText}";

    /// <param name="now">Injected for tests.</param>
    public static DeviceItem From(Device d, DateTimeOffset now)
    {
        var (text, kind) = d switch
        {
            { isSelf: true } => ("This PC", StatusKind.Neutral),
            { status: DeviceStatus.Lan } => ("Connected (same network)", StatusKind.Good),
            { status: DeviceStatus.Relay } => ("Connected (via relay)", StatusKind.Good),
            { online: true } => ("Online, not connected yet", StatusKind.Warning),
            _ => ("Offline", StatusKind.Bad),
        };
        var lastSeen = d.isSelf || d.status != DeviceStatus.Offline ? "" : LastSeen(d.lastSeenMs, now);
        return new DeviceItem(d.nodeId, d.name, PlatformName(d.platform), text, kind, lastSeen, d.isSelf, !d.isSelf);
    }

    public static string PlatformName(Platform p) => p switch
    {
        Platform.Mac => "Mac",
        Platform.Win => "Windows",
        Platform.Cli => "Command line",
        _ => p.ToString(),
    };

    public static string LastSeen(ulong lastSeenMs, DateTimeOffset now)
    {
        if (lastSeenMs == 0) return "Never seen";
        var ago = now - DateTimeOffset.FromUnixTimeMilliseconds((long)lastSeenMs);
        return ago switch
        {
            { TotalMinutes: < 1 } => "Seen just now",
            { TotalHours: < 1 } => $"Seen {(int)ago.TotalMinutes} min ago",
            { TotalDays: < 1 } => $"Seen {(int)ago.TotalHours} h ago",
            _ => $"Seen {(int)ago.TotalDays} d ago",
        };
    }
}
