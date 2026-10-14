using System.Collections.Concurrent;
using System.Diagnostics;

namespace ClipSpike.SelfTest;

/// <summary>WM_CLIPBOARDUPDATE delivery, sensitive-format decisions and self-echo detection.</summary>
internal sealed class ListenerChecks(Report report) : IDisposable
{
    private readonly StaThread _sta = new("listener");
    private readonly BlockingCollection<(uint Seq, List<ClipFormat>? Formats, SkipReason Reason)> _events = new();
    private ClipboardListener? _listener;

    public void Run()
    {
        _listener = _sta.Invoke(() =>
        {
            var l = new ClipboardListener();
            l.Changed += seq =>
            {
                var (formats, reason) = SensitiveFormatFilter.Inspect();
                _events.Add((seq, formats, reason));
            };
            return l;
        });

        Case("plain text (in-proc writer)", SkipReason.None, () => ClipboardWriter.Write("hello"));
        Case("plain text (clip.exe, other process)", SkipReason.None, () =>
        {
            var p = Process.Start(new ProcessStartInfo("cmd.exe", "/c echo spike| clip") { CreateNoWindow = true, UseShellExecute = false })!;
            p.WaitForExit();
        });
        Case("ExcludeClipboardContentFromMonitorProcessing", SkipReason.ExcludeFromMonitor,
            () => ClipboardWriter.Write("secret", (SensitiveFormatFilter.ExcludeFromMonitor, 0)));
        Case("CanIncludeInClipboardHistory=0", SkipReason.NoClipboardHistory,
            () => ClipboardWriter.Write("secret", (SensitiveFormatFilter.CanIncludeInHistory, 0)));
        Case("CanIncludeInClipboardHistory=1", SkipReason.None,
            () => ClipboardWriter.Write("ok", (SensitiveFormatFilter.CanIncludeInHistory, 1)));
        Case("CanUploadToCloudClipboard=0", SkipReason.NoCloudUpload,
            () => ClipboardWriter.Write("secret", (SensitiveFormatFilter.CanUploadToCloud, 0)));
        Case("Clipboard Viewer Ignore (KeePass style)", SkipReason.ViewerIgnore,
            () => ClipboardWriter.Write("secret", (SensitiveFormatFilter.ViewerIgnore, 0)));
        Case("own VirtualFileDataObject (FsyncOrigin)", SkipReason.OwnEcho, () => _sta.Invoke(() =>
            OleClipboard.Set(new VirtualFileDataObject([FakeRemote.File("echo.bin", 10, 1, TimeSpan.Zero)]))));

        // Burst: 50 writes back to back; every one must be seen (sequence numbers advance).
        Drain();
        var seqs = new HashSet<uint>();
        for (var i = 0; i < 50; i++)
        {
            ClipboardWriter.Write($"burst {i}");
            if (_events.TryTake(out var e, TimeSpan.FromSeconds(2))) seqs.Add(e.Seq);
        }
        while (_events.TryTake(out var e, TimeSpan.FromMilliseconds(300))) seqs.Add(e.Seq);
        report.Check("listener: 50 rapid writes", seqs.Count >= 50, $"{seqs.Count} distinct updates");
    }

    private void Case(string name, SkipReason expected, Action write)
    {
        Drain();
        var sw = Stopwatch.StartNew();
        write();
        // One write can produce more than one update (OLE delayed render); use the last one.
        (uint Seq, List<ClipFormat>? Formats, SkipReason Reason)? last = null;
        if (_events.TryTake(out var first, TimeSpan.FromSeconds(3))) last = first;
        var latency = sw.ElapsedMilliseconds;
        while (_events.TryTake(out var more, TimeSpan.FromMilliseconds(200))) last = more;
        if (last is not { } ev)
        {
            report.Check($"filter: {name}", false, "no WM_CLIPBOARDUPDATE within 3 s");
            return;
        }
        var formats = ev.Formats is null ? "unreadable" : string.Join(", ", ev.Formats.Select(f => $"{f.Name}({f.Size})"));
        report.Check($"filter: {name}", ev.Reason == expected, $"{ev.Reason} in {latency} ms; formats: {formats}");
    }

    private void Drain()
    {
        Thread.Sleep(100);
        while (_events.TryTake(out _)) { }
    }

    public void Dispose()
    {
        if (_listener is not null) _sta.Invoke(_listener.Dispose);
        _sta.Dispose();
    }
}
