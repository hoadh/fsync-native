using System.Runtime.InteropServices;
using Fsync.Interop.Shell;

namespace Fsync.App.Files;

/// <summary>
/// Explorer "Send to > fsync": a shortcut in shell:sendto runs <c>Fsync.exe --send &lt;paths&gt;</c>.
/// A running instance gets the arguments through AppInstance redirection (same user only).
/// </summary>
public sealed class SendToShortcut(string exePath, string? sendToDir = null)
{
    public const string Flag = "--send";
    private readonly string _dir = sendToDir ?? KnownFolders.SendToPath();

    public string LinkPath => Path.Combine(_dir, "fsync (other device).lnk");

    /// <summary>True when our shortcut exists and points at this exe.</summary>
    public bool IsInstalled => ShellLink.Read(LinkPath) is { } l && string.Equals(l.Target, exePath, StringComparison.OrdinalIgnoreCase);

    public void Set(bool installed)
    {
        if (installed) ShellLink.Create(LinkPath, exePath, Flag, "Send to another device with fsync", exePath);
        else if (File.Exists(LinkPath)) File.Delete(LinkPath);
    }
}

/// <summary>Parses a command line for <c>--send p1 p2 ...</c> and keeps only existing absolute paths.</summary>
public static class SendToArgs
{
    /// <summary>Paths after --send (null when the flag is absent).</summary>
    public static List<string>? Parse(IReadOnlyList<string> args)
    {
        var i = args.ToList().FindIndex(a => string.Equals(a, SendToShortcut.Flag, StringComparison.OrdinalIgnoreCase));
        if (i < 0) return null;
        return args.Skip(i + 1)
            // Local paths only: a UNC or \\?\ path could make us touch the network or a device.
            .Where(p => Path.IsPathFullyQualified(p) && !p.StartsWith(@"\\", StringComparison.Ordinal) && (File.Exists(p) || Directory.Exists(p)))
            .Select(Path.GetFullPath)
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToList();
    }

    /// <summary>Splits a raw command line (AppInstance activation arguments) like the C runtime does.</summary>
    public static string[] Split(string commandLine)
    {
        if (string.IsNullOrWhiteSpace(commandLine)) return [];
        var argv = CommandLineToArgvW(commandLine, out var n);
        if (argv == IntPtr.Zero) return [];
        try
        {
            var result = new string[n];
            for (var i = 0; i < n; i++) result[i] = Marshal.PtrToStringUni(Marshal.ReadIntPtr(argv, i * IntPtr.Size)) ?? "";
            return result;
        }
        finally { LocalFree(argv); }
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr CommandLineToArgvW(string cmd, out int count);
    [DllImport("kernel32.dll")] private static extern IntPtr LocalFree(IntPtr mem);
}
