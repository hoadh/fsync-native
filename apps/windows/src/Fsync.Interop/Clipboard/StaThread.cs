using System.Collections.Concurrent;
using Windows.Win32;
using Windows.Win32.UI.WindowsAndMessaging;

namespace Fsync.Interop.Clipboard;

/// <summary>
/// Dedicated STA thread with a Win32 message loop. It owns the clipboard listener window and
/// our OLE clipboard entries, so delayed renders (which block on the network) never freeze the
/// UI thread. <see cref="Invoke{T}"/> / <see cref="Post"/> run work on it.
/// </summary>
public sealed class StaThread : IDisposable
{
    private const uint WM_APP_WORK = 0x8001; // WM_APP + 1
    private readonly Thread _thread;
    private readonly BlockingCollection<Action> _work = [];
    private readonly TaskCompletionSource<uint> _started = new();
    private uint _threadId;

    public StaThread(string name)
    {
        _thread = new Thread(Run) { IsBackground = true, Name = name };
        _thread.SetApartmentState(ApartmentState.STA);
        _thread.Start();
        _threadId = _started.Task.GetAwaiter().GetResult();
    }

    public bool IsCurrent => Thread.CurrentThread == _thread;

    private unsafe void Run()
    {
        try { OleClipboard.InitializeSta(); }
        catch (Exception e) { _started.TrySetException(e); return; }
        MSG msg;
        // Creates the thread's message queue before anyone posts to it.
        PInvoke.PeekMessage(&msg, default, 0, 0, PEEK_MESSAGE_REMOVE_TYPE.PM_NOREMOVE);
        _started.SetResult(PInvoke.GetCurrentThreadId());
        while (PInvoke.GetMessage(&msg, default, 0, 0) > 0)
        {
            if (msg.hwnd.IsNull && msg.message == WM_APP_WORK)
            {
                while (_work.TryTake(out var a)) a();
                continue;
            }
            PInvoke.TranslateMessage(&msg);
            PInvoke.DispatchMessage(&msg);
        }
        OleClipboard.Uninitialize();
    }

    /// <summary>Queues <paramref name="a"/>; exceptions are traced, never thrown on the STA.</summary>
    public void Post(Action a)
    {
        _work.Add(() =>
        {
            try { a(); }
            catch (Exception e) { System.Diagnostics.Trace.TraceError($"{_thread.Name}: {e.GetType().Name}: {e.Message}"); }
        });
        PInvoke.PostThreadMessage(_threadId, WM_APP_WORK, default, default);
    }

    public Task<T> InvokeAsync<T>(Func<T> f)
    {
        var tcs = new TaskCompletionSource<T>(TaskCreationOptions.RunContinuationsAsynchronously);
        if (IsCurrent)
        {
            try { tcs.SetResult(f()); } catch (Exception e) { tcs.SetException(e); }
            return tcs.Task;
        }
        _work.Add(() =>
        {
            try { tcs.SetResult(f()); } catch (Exception e) { tcs.SetException(e); }
        });
        PInvoke.PostThreadMessage(_threadId, WM_APP_WORK, default, default);
        return tcs.Task;
    }

    public Task InvokeAsync(Action a) => InvokeAsync(() => { a(); return 0; });
    public T Invoke<T>(Func<T> f) => InvokeAsync(f).GetAwaiter().GetResult();
    public void Invoke(Action a) => InvokeAsync(a).GetAwaiter().GetResult();

    public void Dispose()
    {
        if (_threadId == 0) return;
        PInvoke.PostThreadMessage(_threadId, PInvoke.WM_QUIT, default, default);
        _thread.Join(TimeSpan.FromSeconds(5));
        _threadId = 0;
    }
}
