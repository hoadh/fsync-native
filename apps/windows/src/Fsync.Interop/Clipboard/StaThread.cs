using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using Windows.Win32;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Fsync.Interop.Clipboard;

/// <summary>
/// Dedicated STA thread with a Win32 message loop. It owns the clipboard listener window and
/// our OLE clipboard entries, so delayed renders (which block on the network) never freeze the
/// UI thread. Work is queued and signalled through an event the loop waits on together with
/// messages (MsgWaitForMultipleObjectsEx), so a wake-up cannot be lost in a nested modal loop.
/// On dispose, queued work that never ran fails with ObjectDisposedException.
/// </summary>
public sealed class StaThread : IDisposable
{
    private const uint QS_ALLINPUT = 0x04FF, MWMO_INPUTAVAILABLE = 0x0004, INFINITE = 0xFFFFFFFF;
    private readonly Thread _thread;
    private readonly ConcurrentQueue<(Action Run, Action<Exception> Fail)> _work = new();
    private readonly AutoResetEvent _wake = new(false);
    private readonly TaskCompletionSource _started = new();
    private volatile bool _stopping;

    public StaThread(string name)
    {
        _thread = new Thread(Run) { IsBackground = true, Name = name };
        _thread.SetApartmentState(ApartmentState.STA);
        _thread.Start();
        _started.Task.GetAwaiter().GetResult();
    }

    public bool IsCurrent => Thread.CurrentThread == _thread;

    private unsafe void Run()
    {
        try { OleClipboard.InitializeSta(); }
        catch (Exception e) { _started.TrySetException(e); return; }
        _started.SetResult();
        var handle = _wake.SafeWaitHandle.DangerousGetHandle();
        MSG msg;
        while (!_stopping)
        {
            MsgWaitForMultipleObjectsEx(1, ref handle, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            Drain();
            while (!_stopping && PInvoke.PeekMessage(&msg, default, 0, 0, PEEK_MESSAGE_REMOVE_TYPE.PM_REMOVE))
            {
                if (msg.message == PInvoke.WM_QUIT) { _stopping = true; break; }
                PInvoke.TranslateMessage(&msg);
                PInvoke.DispatchMessage(&msg);
            }
        }
        FailPending();
        OleClipboard.Uninitialize();
    }

    private void Drain()
    {
        while (!_stopping && _work.TryDequeue(out var w)) w.Run();
    }

    private void FailPending()
    {
        while (_work.TryDequeue(out var w)) w.Fail(new ObjectDisposedException(_thread.Name));
    }

    private void Enqueue(Action run, Action<Exception> fail)
    {
        if (_stopping) { fail(new ObjectDisposedException(_thread.Name)); return; }
        _work.Enqueue((run, fail));
        _wake.Set();
        if (_stopping) FailPending(); // raced with Dispose
    }

    /// <summary>Queues <paramref name="a"/>; exceptions are traced, never thrown on the STA.</summary>
    public void Post(Action a) => Enqueue(() =>
    {
        try { a(); }
        catch (Exception e) { System.Diagnostics.Trace.TraceError($"{_thread.Name}: {e.GetType().Name}"); }
    }, _ => { });

    public Task<T> InvokeAsync<T>(Func<T> f)
    {
        var tcs = new TaskCompletionSource<T>(TaskCreationOptions.RunContinuationsAsynchronously);
        void Run()
        {
            try { tcs.TrySetResult(f()); } catch (Exception e) { tcs.TrySetException(e); }
        }
        if (IsCurrent) Run();
        else Enqueue(Run, e => tcs.TrySetException(e));
        return tcs.Task;
    }

    public Task InvokeAsync(Action a) => InvokeAsync(() => { a(); return 0; });
    public T Invoke<T>(Func<T> f) => InvokeAsync(f).GetAwaiter().GetResult();
    public void Invoke(Action a) => InvokeAsync(a).GetAwaiter().GetResult();

    /// <summary>Runs <paramref name="a"/> on the thread, waiting at most <paramref name="timeout"/>; false on timeout.</summary>
    public bool TryInvoke(Action a, TimeSpan timeout)
    {
        var t = InvokeAsync(a);
        try { return t.Wait(timeout); }
        catch (AggregateException e) when (e.InnerException is not null) { throw e.InnerException; }
    }

    /// <summary>Stops the loop; waits up to 5 s for it (not when called on the thread itself).</summary>
    public void Dispose() => Dispose(TimeSpan.FromSeconds(5));

    /// <summary>Stops the loop, waiting at most <paramref name="join"/> (a blocked work item is left behind).</summary>
    public void Dispose(TimeSpan join)
    {
        if (_stopping) return;
        _stopping = true;
        _wake.Set();
        if (!IsCurrent) _thread.Join(join);
        FailPending();
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint MsgWaitForMultipleObjectsEx(uint count, ref IntPtr handles, uint ms, uint wakeMask, uint flags);
}
