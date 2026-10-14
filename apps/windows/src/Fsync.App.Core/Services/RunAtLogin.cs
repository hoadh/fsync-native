using Microsoft.Win32;

namespace Fsync.App.Services;

/// <summary>Start with Windows for an unpackaged app: HKCU\...\Run value (no admin rights).</summary>
public interface IRunAtLogin
{
    bool IsEnabled { get; }
    void Set(bool enabled);
}

public sealed class RunAtLogin(string valueName, string command, string keyPath = RunAtLogin.RunKey) : IRunAtLogin
{
    public const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";

    /// <summary>Command for this exe, started hidden in the tray.</summary>
    public static string CommandFor(string exePath) => $"\"{exePath}\" --background";

    public bool IsEnabled
    {
        get
        {
            using var key = Registry.CurrentUser.OpenSubKey(keyPath);
            return key?.GetValue(valueName) is string v && string.Equals(v, command, StringComparison.OrdinalIgnoreCase);
        }
    }

    public void Set(bool enabled)
    {
        using var key = Registry.CurrentUser.CreateSubKey(keyPath, writable: true);
        if (enabled) key.SetValue(valueName, command, RegistryValueKind.String);
        else key.DeleteValue(valueName, throwOnMissingValue: false);
    }
}
