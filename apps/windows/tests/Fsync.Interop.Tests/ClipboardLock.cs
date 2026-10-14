namespace Fsync.Interop.Tests;

/// <summary>
/// Collection fixture: holds a machine-wide semaphore while clipboard tests run, so test
/// assemblies that `dotnet test` runs in parallel processes never share the clipboard.
/// </summary>
public sealed class ClipboardLock : IDisposable
{
    private readonly Semaphore _gate = new(1, 1, @"Local\fsync-clipboard-tests");

    public ClipboardLock()
    {
        if (!_gate.WaitOne(TimeSpan.FromMinutes(5))) throw new TimeoutException("another test run holds the clipboard");
    }

    public void Dispose()
    {
        _gate.Release();
        _gate.Dispose();
    }
}
