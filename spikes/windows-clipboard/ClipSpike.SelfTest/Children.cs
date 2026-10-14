using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using System.Security.Cryptography;
using System.Text;

namespace ClipSpike.SelfTest;

/// <summary>Child-process modes: the "other app" side of each check.</summary>
internal static class Children
{
    /// <summary>--read: OleGetClipboard, descriptor + every FileContents stream, one line per file.</summary>
    public static int Read()
    {
        using var sta = new StaThread("reader");
        return sta.Invoke(() =>
        {
            try
            {
                var obj = OleClipboard.Get();
                var names = ReadDescriptor(obj);
                Console.WriteLine($"COUNT {names.Count}");
                for (var i = 0; i < names.Count; i++)
                {
                    var sw = Stopwatch.StartNew();
                    var (size, hash) = ReadStream(obj, i);
                    Console.WriteLine($"FILE {i} {size} {hash} {sw.ElapsedMilliseconds}ms {names[i]}");
                }
                return 0;
            }
            catch (COMException e)
            {
                Console.WriteLine($"ERROR 0x{e.HResult:X8} {e.Message.Trim()}");
                return 2;
            }
        });
    }

    private static List<string> ReadDescriptor(IDataObject obj)
    {
        var fmt = Format(ShellFormats.FileDescriptorW, TYMED.TYMED_HGLOBAL, -1);
        obj.GetData(ref fmt, out var med);
        // Cross-process the HGLOBAL is a moveable copy: lock it, never use the handle as a pointer.
        var p = GlobalLock(med.unionmember);
        try
        {
            if (p == IntPtr.Zero) throw new COMException("GlobalLock failed", unchecked((int)0x80004005));
            var count = Marshal.ReadInt32(p);
            var names = new List<string>();
            for (var i = 0; i < count; i++)
                names.Add(Marshal.PtrToStringUni(p + 4 + i * 592 + 72) ?? "");
            return names;
        }
        finally
        {
            if (p != IntPtr.Zero) GlobalUnlock(med.unionmember);
            ReleaseStgMedium(ref med);
        }
    }

    private static (long size, string hash) ReadStream(IDataObject obj, int index)
    {
        var fmt = Format(ShellFormats.FileContents, TYMED.TYMED_ISTREAM, index);
        obj.GetData(ref fmt, out var med);
        try
        {
            var stream = (IStream)Marshal.GetObjectForIUnknown(med.unionmember);
            using var h = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
            var buf = new byte[1 << 20];
            var pRead = Marshal.AllocHGlobal(4);
            long total = 0;
            try
            {
                while (true)
                {
                    stream.Read(buf, buf.Length, pRead);
                    var n = Marshal.ReadInt32(pRead);
                    if (n == 0) break;
                    h.AppendData(buf, 0, n);
                    total += n;
                }
            }
            finally
            {
                Marshal.FreeHGlobal(pRead);
            }
            return (total, Convert.ToHexString(h.GetHashAndReset()));
        }
        finally
        {
            ReleaseStgMedium(ref med);
        }
    }

    /// <summary>--shell-paste dir count: the shell's Paste verb on a folder (Explorer's copy engine).</summary>
    /// <remarks>
    /// The copy engine runs on its own threads in this process and pre-sizes target files, so
    /// file sizes say nothing about progress. The parent decides when it is over (EndOperation
    /// on its data object, or an error dialog we report) and writes a line to our stdin.
    /// Prints "WINDOW title" for each new visible window, then "PASTED files bytes".
    /// </remarks>
    public static int ShellPaste(string dir)
    {
        using var sta = new StaThread("shell");
        sta.Invoke(() =>
        {
            var shellType = Type.GetTypeFromProgID("Shell.Application") ?? throw new InvalidOperationException("no Shell.Application");
            dynamic shell = Activator.CreateInstance(shellType)!;
            shell.NameSpace(dir).Self.InvokeVerb("Paste");
        });
        var done = Task.Run(Console.ReadLine);
        var seen = new HashSet<string>();
        while (!done.Wait(200))
        {
            foreach (var w in OwnVisibleWindows())
                if (seen.Add(w)) { Console.WriteLine($"WINDOW {w}"); Console.Out.Flush(); }
        }
        var files = Directory.GetFiles(dir);
        Console.WriteLine($"PASTED {files.Length} {files.Sum(f => new FileInfo(f).Length)}");
        return 0;
    }

    /// <summary>--own-and-exit none|flush|clear: owns a 1 MiB entry until stdin says "exit".</summary>
    public static int OwnAndExit(string policy)
    {
        using var sta = new StaThread("owner");
        var obj = sta.Invoke(() =>
        {
            var o = new VirtualFileDataObject([FakeRemote.File("exit-test.bin", 1 << 20, 3, TimeSpan.Zero)]);
            OleClipboard.Set(o);
            return o;
        });
        Console.WriteLine("OWNED");
        Console.Out.Flush();
        Console.ReadLine();
        sta.Invoke(() =>
        {
            if (policy == "flush") OleClipboard.Flush();
            else if (policy == "clear") OleClipboard.ClearIfCurrent(obj);
        });
        Console.WriteLine($"EXIT {policy}");
        return 0;
    }

    private static FORMATETC Format(string name, TYMED tymed, int lindex) => new()
    {
        cfFormat = unchecked((short)SensitiveFormatFilter.Register(name)),
        dwAspect = DVASPECT.DVASPECT_CONTENT,
        lindex = lindex,
        tymed = tymed,
    };

    [DllImport("ole32.dll")] private static extern void ReleaseStgMedium(ref STGMEDIUM medium);
    [DllImport("kernel32.dll")] private static extern IntPtr GlobalLock(IntPtr h);
    [DllImport("kernel32.dll")] private static extern bool GlobalUnlock(IntPtr h);

    private delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);

    /// <summary>Titles of this process's visible top-level windows (shell progress / error dialogs).</summary>
    private static List<string> OwnVisibleWindows()
    {
        var me = (uint)Environment.ProcessId;
        var titles = new List<string>();
        EnumWindows((h, _) =>
        {
            GetWindowThreadProcessId(h, out var pid);
            if (pid == me && IsWindowVisible(h))
            {
                var sb = new StringBuilder(256);
                GetWindowText(h, sb, sb.Capacity);
                titles.Add(sb.ToString());
            }
            return true;
        }, IntPtr.Zero);
        return titles;
    }

    /// <summary>Starts this exe in a child mode; returns its stdout once it exits.</summary>
    public static (int Exit, string Output) Run(TimeSpan timeout, params string[] args)
    {
        using var p = Start(args);
        var output = p.StandardOutput.ReadToEndAsync();
        if (!p.WaitForExit(timeout)) { p.Kill(); return (-1, "TIMEOUT " + output.Result); }
        return (p.ExitCode, output.Result.Trim());
    }

    public static Process Start(params string[] args)
    {
        var psi = new ProcessStartInfo(Environment.ProcessPath!)
        {
            RedirectStandardOutput = true,
            RedirectStandardInput = true,
            UseShellExecute = false,
            StandardOutputEncoding = Encoding.UTF8,
        };
        foreach (var a in args) psi.ArgumentList.Add(a);
        return Process.Start(psi)!;
    }
}
