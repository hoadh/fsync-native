using System.Diagnostics;

namespace ClipSpike.SelfTest;

/// <summary>Lazy virtual-file paste: cross-process read, shell paste, cancel, owner exit.</summary>
internal sealed class PasteChecks(Report report, bool large) : IDisposable
{
    private readonly StaThread _owner = new("owner");
    private readonly string _root = Path.Combine(Path.GetTempPath(), "fsync-clipspike-" + Guid.NewGuid().ToString("N")[..8]);

    private static readonly TimeSpan Latency = TimeSpan.FromMilliseconds(2); // per range request
    private static readonly (string Name, long Size, byte Seed)[] Multi =
        [("notes.txt", 1_234, 11), ("photo.jpg", 5L << 20, 22), ("empty.dat", 0, 33)];

    public void Run()
    {
        Directory.CreateDirectory(_root);
        CrossProcessRead();
        ShellPaste("shell paste: 3 files", Multi);
        if (large) ShellPaste("shell paste: 200 MiB", [("big.bin", 200L << 20, 44)]);
        ShellPaste("shell paste: 20 MiB, 50 ms per read (relay-like RTT)", [("rtt.bin", 20L << 20, 66)], TimeSpan.FromMilliseconds(50));
        Cancel();
        foreach (var policy in new[] { "none", "flush", "clear", "kill" }) OwnerExit(policy);
    }

    private VirtualFileDataObject Offer((string Name, long Size, byte Seed)[] files, Func<RangeReader, RangeReader>? wrap = null, TimeSpan? latency = null)
    {
        var vfs = files.Select(f =>
        {
            var v = FakeRemote.File(f.Name, f.Size, f.Seed, latency ?? Latency);
            return wrap is null ? v : v with { Reader = wrap(v.Reader) };
        }).ToList();
        return _owner.Invoke(() =>
        {
            var o = new VirtualFileDataObject(vfs);
            OleClipboard.Set(o);
            return o;
        });
    }

    private void CrossProcessRead()
    {
        var obj = Offer(Multi);
        var (exit, output) = Children.Run(TimeSpan.FromMinutes(2), "--read");
        var lines = output.Split('\n', StringSplitOptions.TrimEntries);
        var ok = exit == 0 && lines.FirstOrDefault() == $"COUNT {Multi.Length}";
        for (var i = 0; i < Multi.Length && ok; i++)
        {
            var f = Multi[i];
            var parts = lines.ElementAtOrDefault(i + 1)?.Split(' ') ?? [];
            ok = parts.Length >= 4 && parts[2] == f.Size.ToString() && parts[3] == FakeRemote.Hash(f.Size, f.Seed);
        }
        var reads = obj.Streams.Sum(s => s.Reads);
        report.Check("cross-process OleGetClipboard read (3 files, IStream proxied)", ok, $"exit {exit}, {reads} range reads; {string.Join(" | ", lines)}");
    }

    private void ShellPaste(string name, (string Name, long Size, byte Seed)[] files, TimeSpan? latency = null)
    {
        RemoteFileStream.ResetConcurrency();
        var obj = Offer(files, latency: latency);
        var log = new List<string>();
        obj.Log += s => { lock (log) log.Add(s); };
        var dest = Directory.CreateDirectory(Path.Combine(_root, "paste-" + Guid.NewGuid().ToString("N")[..6])).FullName;
        var total = files.Sum(f => f.Size);
        var (exit, output, elapsed) = Paste(obj, dest, TimeSpan.FromMinutes(15));

        var ok = exit == 0 && obj.OperationResult == 0;
        foreach (var f in files)
        {
            var path = Path.Combine(dest, f.Name);
            ok &= File.Exists(path) && new FileInfo(path).Length == f.Size && FakeRemote.HashFile(path) == FakeRemote.Hash(f.Size, f.Seed);
        }
        var streams = obj.Streams;
        var reads = streams.Sum(s => s.Reads);
        var avgChunk = reads == 0 ? 0 : streams.Sum(s => s.BytesServed) / reads;
        var mibps = total / (1024.0 * 1024) / Math.Max(elapsed.TotalSeconds, 0.001);
        report.Check(name, ok,
            $"{output}; {elapsed.TotalMilliseconds:F0} ms to EndOperation, {reads} reads, avg {avgChunk / 1024} KiB, ~{mibps:F0} MiB/s " +
            $"(fake remote: {(latency ?? Latency).TotalMilliseconds} ms per read, max {RemoteFileStream.MaxConcurrentReads} in flight); async op started={obj.OperationStarted}, " +
            $"EndOperation hr={(obj.OperationResult is { } hr ? $"0x{hr:X8}" : "none")}, PerformedDropEffect={obj.PerformedDropEffect?.ToString() ?? "none"}, PasteSucceeded={obj.PasteSucceeded?.ToString() ?? "none"}");
        lock (log) report.Info($"{name} call log: {string.Join("; ", log.Distinct().Take(12))}");
    }

    /// <summary>Remote read starts failing after ~20 MiB (peer gone / user cancel): the paste must end, not hang.</summary>
    private void Cancel()
    {
        long served = 0;
        var obj = Offer([("cancel.bin", 200L << 20, 55)], inner => (off, len) =>
        {
            if (Interlocked.Add(ref served, len) > 20L << 20) throw new IOException("peer went away");
            return inner(off, len);
        });
        var dest = Directory.CreateDirectory(Path.Combine(_root, "cancel")).FullName;
        var (exit, output, elapsed) = Paste(obj, dest, TimeSpan.FromMinutes(2));
        var leftovers = Directory.GetFiles(dest).Select(f => $"{Path.GetFileName(f)}={new FileInfo(f).Length}");
        var ended = obj.OperationEnded.IsSet || output.Contains("WINDOW Error", StringComparison.Ordinal);
        report.Check("shell paste: remote failure mid-file ends the paste (no hang)", exit == 0 && ended,
            $"{output.Replace('\n', '|')}; {elapsed.TotalSeconds:F1} s; EndOperation hr={(obj.OperationResult is { } hr ? $"0x{hr:X8}" : "none")}; files left: [{string.Join(", ", leftovers)}]");
    }

    /// <summary>
    /// Runs the shell-paste child until EndOperation fires on <paramref name="obj"/>, the child
    /// shows an error dialog, or <paramref name="timeout"/>; then tells it to finish.
    /// </summary>
    private static (int Exit, string Output, TimeSpan Elapsed) Paste(VirtualFileDataObject obj, string dest, TimeSpan timeout)
    {
        var sw = Stopwatch.StartNew();
        using var child = Children.Start("--shell-paste", dest);
        var lines = new List<string>();
        var errorDialog = new ManualResetEventSlim();
        child.OutputDataReceived += (_, e) =>
        {
            if (e.Data is null) return;
            lock (lines) lines.Add(e.Data);
            if (e.Data.StartsWith("WINDOW Error", StringComparison.Ordinal)) errorDialog.Set();
        };
        child.BeginOutputReadLine();
        var which = WaitHandle.WaitAny([obj.OperationEnded.WaitHandle, errorDialog.WaitHandle], timeout);
        var elapsed = sw.Elapsed;
        if (which == WaitHandle.WaitTimeout) lock (lines) lines.Add("TIMEOUT");
        child.StandardInput.WriteLine("done");
        if (!child.WaitForExit(30_000)) { child.Kill(); lock (lines) lines.Add("KILLED"); }
        child.WaitForExit(); // flush async output
        lock (lines) return (child.ExitCode, string.Join("\n", lines), elapsed);
    }

    /// <summary>What another app sees after the owning process goes away (policy = what it did on exit).</summary>
    private void OwnerExit(string policy)
    {
        using var child = Children.Start("--own-and-exit", policy == "kill" ? "none" : policy);
        var ready = child.StandardOutput.ReadLine();
        if (policy == "kill") child.Kill();
        else child.StandardInput.WriteLine("exit");
        child.WaitForExit(10_000);
        var (exit, output) = Children.Run(TimeSpan.FromSeconds(30), "--read");
        var formats = SensitiveFormatFilter.Inspect().Formats?.Select(f => f.Name) ?? ["unreadable"];
        report.Info($"owner exit '{policy}' ({ready}): reader exit {exit}: {output.Replace('\n', '|')}; formats now: [{string.Join(", ", formats)}]");
    }

    public void Dispose()
    {
        _owner.Dispose();
        try { Directory.Delete(_root, recursive: true); } catch (IOException) { } catch (UnauthorizedAccessException) { }
    }
}
