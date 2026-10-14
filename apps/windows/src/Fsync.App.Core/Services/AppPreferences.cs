using System.Text.Json;
using System.Text.Json.Serialization;
using Fsync.App.Clipboard;

namespace Fsync.App.Services;

/// <summary>
/// Windows-only preferences that core does not store (core keeps clip mode, retention, limits,
/// denylist, auto-accept). JSON next to the secrets; a missing or broken file means defaults.
/// </summary>
public sealed class AppPreferences
{
    private static readonly JsonSerializerOptions Json = new() { WriteIndented = true, Converters = { new JsonStringEnumConverter() } };

    public FileClipStrategy FileStrategy { get; set; } = FileClipStrategy.Lazy;
    /// <summary>Ctrl+Alt+C "send clipboard now" and Ctrl+Alt+V "paste latest from another device".</summary>
    public bool HotkeysEnabled { get; set; } = true;
    /// <summary>Show a tray notification when a clip arrives in manual mode or files are offered.</summary>
    public bool Notifications { get; set; } = true;

    [JsonIgnore] public string? Path { get; private set; }

    public static AppPreferences Load(string path)
    {
        AppPreferences? p = null;
        try
        {
            if (File.Exists(path)) p = JsonSerializer.Deserialize<AppPreferences>(File.ReadAllText(path), Json);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or JsonException)
        {
            System.Diagnostics.Trace.TraceWarning($"preferences unreadable, using defaults: {e.GetType().Name}");
        }
        p ??= new AppPreferences();
        p.Path = path;
        return p;
    }

    /// <summary>Writes atomically (temp + move). Throws IOException / UnauthorizedAccessException.</summary>
    public void Save()
    {
        if (Path is null) return;
        Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path)!);
        var tmp = Path + ".tmp";
        File.WriteAllText(tmp, JsonSerializer.Serialize(this, Json));
        File.Move(tmp, Path, overwrite: true);
    }
}
