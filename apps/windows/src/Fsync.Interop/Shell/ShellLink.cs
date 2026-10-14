using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using System.Text;

namespace Fsync.Interop.Shell;

/// <summary>.lnk files through the shell's own ShellLink object (IShellLinkW + IPersistFile).</summary>
public static class ShellLink
{
    /// <summary>Creates or replaces a shortcut. Throws COMException / UnauthorizedAccessException.</summary>
    public static void Create(string lnkPath, string target, string arguments, string description, string? icon = null)
    {
        var link = (IShellLinkW)new ShellLinkObject();
        try
        {
            link.SetPath(target);
            link.SetArguments(arguments);
            link.SetDescription(description);
            link.SetWorkingDirectory(Path.GetDirectoryName(target) ?? "");
            if (icon is not null) link.SetIconLocation(icon, 0);
            Directory.CreateDirectory(Path.GetDirectoryName(lnkPath)!);
            ((IPersistFile)link).Save(lnkPath, true);
        }
        finally { Marshal.ReleaseComObject(link); }
    }

    /// <summary>Target and arguments of a shortcut, or null when it is missing or unreadable.</summary>
    public static (string Target, string Arguments)? Read(string lnkPath)
    {
        if (!File.Exists(lnkPath)) return null;
        var link = (IShellLinkW)new ShellLinkObject();
        try
        {
            ((IPersistFile)link).Load(lnkPath, 0);
            var path = new StringBuilder(1024);
            var args = new StringBuilder(4096);
            link.GetPath(path, path.Capacity, IntPtr.Zero, 0);
            link.GetArguments(args, args.Capacity);
            return (path.ToString(), args.ToString());
        }
        catch (COMException) { return null; }
        finally { Marshal.ReleaseComObject(link); }
    }

    [ComImport, Guid("00021401-0000-0000-C000-000000000046")]
    private class ShellLinkObject;

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("000214F9-0000-0000-C000-000000000046")]
    private interface IShellLinkW
    {
        void GetPath([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder file, int cch, IntPtr findData, uint flags);
        void GetIDList(out IntPtr pidl);
        void SetIDList(IntPtr pidl);
        void GetDescription([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int cch);
        void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string name);
        void GetWorkingDirectory([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder dir, int cch);
        void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string dir);
        void GetArguments([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder args, int cch);
        void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string args);
        void GetHotkey(out short hotkey);
        void SetHotkey(short hotkey);
        void GetShowCmd(out int showCmd);
        void SetShowCmd(int showCmd);
        void GetIconLocation([Out, MarshalAs(UnmanagedType.LPWStr)] StringBuilder path, int cch, out int icon);
        void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string path, int icon);
        void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string path, uint reserved);
        void Resolve(IntPtr hwnd, uint flags);
        void SetPath([MarshalAs(UnmanagedType.LPWStr)] string file);
    }
}

/// <summary>Known folder paths the BCL does not expose.</summary>
public static class KnownFolders
{
    private static readonly Guid Downloads = new("374DE290-123F-4565-9164-39C4925E467B");

    /// <summary>The user's Downloads folder (falls back to %USERPROFILE%\Downloads).</summary>
    public static string DownloadsPath()
    {
        if (SHGetKnownFolderPath(Downloads, 0, IntPtr.Zero, out var p) == 0 && p != IntPtr.Zero)
        {
            try { return Marshal.PtrToStringUni(p) ?? Fallback(); }
            finally { Marshal.FreeCoTaskMem(p); }
        }
        return Fallback();
    }

    private static string Fallback() => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads");

    /// <summary>The user's Send To folder (shell:sendto).</summary>
    public static string SendToPath() => Environment.GetFolderPath(Environment.SpecialFolder.SendTo);

    [DllImport("shell32.dll")]
    private static extern int SHGetKnownFolderPath([MarshalAs(UnmanagedType.LPStruct)] Guid id, uint flags, IntPtr token, out IntPtr path);
}
