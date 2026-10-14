using System.Diagnostics;
using System.Runtime.InteropServices;
using FfiSpike;

// Prints one line per check; exit code 1 on any failure.
var failed = false;
void Check(string name, bool ok, string detail)
{
    Console.WriteLine($"{(ok ? "PASS" : "FAIL")} {name}: {detail}");
    failed |= !ok;
}

Console.WriteLine($"process arch: {RuntimeInformation.ProcessArchitecture}");

// 1. 10k callbacks, small payload, from a Rust-spawned thread.
foreach (var (count, len) in new[] { (10_000UL, 16U), (10_000UL, 4096U) })
{
    var sink = new CountingSink();
    var sw = Stopwatch.StartNew();
    Spike.StressCallbacks(sink, count, len);
    sw.Stop();
    GC.Collect();
    GC.WaitForPendingFinalizers();
    Check($"callbacks {count} x {len} B", sink.Count == (long)count && sink.InOrder && sink.OffMainThread,
        $"got {sink.Count}, ordered={sink.InOrder}, offMain={sink.OffMainThread}, {sw.Elapsed.TotalMilliseconds:F1} ms");
}

// 2. Repeated runs with GC pressure: handles must not leak or be collected early.
{
    var total = 0L;
    for (var i = 0; i < 20; i++)
    {
        var sink = new CountingSink();
        Spike.StressCallbacks(sink, 1_000, 64);
        total += sink.Count;
        GC.Collect();
    }
    Check("20 x 1k callbacks with GC between", total == 20_000, $"got {total}");
}

// 3. Echo timing (C# -> RustBuffer -> Vec -> RustBuffer -> C#). Gate: 10 MiB < 20 ms.
foreach (var mib in new[] { 1, 4, 10 })
{
    var data = new byte[mib * 1024 * 1024];
    new Random(1).NextBytes(data);
    Spike.Echo(data); // warm-up
    var best = double.MaxValue;
    byte[] back = [];
    for (var i = 0; i < 10; i++)
    {
        var sw = Stopwatch.StartNew();
        back = Spike.Echo(data);
        sw.Stop();
        best = Math.Min(best, sw.Elapsed.TotalMilliseconds);
    }
    var ok = back.AsSpan().SequenceEqual(data) && (mib != 10 || best < 20);
    Check($"{mib} MiB echo (best of 10){(mib == 10 ? ", gate < 20 ms" : "")}", ok, $"{best:F2} ms");
}

return failed ? 1 : 0;

sealed class CountingSink : Sink
{
    private readonly int _mainThread = Environment.CurrentManagedThreadId;
    private long _count;
    private ulong _next;
    public bool InOrder { get; private set; } = true;
    public bool OffMainThread { get; private set; } = true;
    public long Count => Interlocked.Read(ref _count);

    public void OnEvent(ulong seq, byte[] payload)
    {
        if (seq != _next) InOrder = false;
        _next = seq + 1;
        if (Environment.CurrentManagedThreadId == _mainThread) OffMainThread = false;
        Interlocked.Increment(ref _count);
    }
}
