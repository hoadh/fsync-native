using System.Collections.Concurrent;
using Windows.Win32;
using Windows.Win32.UI.WindowsAndMessaging;

namespace ClipSpike;

/// <summary>
/// Dedicated STA thread with a Win32 message loop: owns the listener window and the OLE
/// clipboard entry (the "dedicated Win32 helper thread" fallback from phase 05, and the
/// harness for console self-tests). <see cref="Invoke{T}"/> runs work on it.
/// </summary>
public sealed class StaThread : IDisposable
{
    private const uint WM_APP_WORK = 0x8001; // WM_APP + 1
    private readonly Thread _thread;
    private readonly BlockingCollection<Action> _work = new();
    private readonly TaskCompletionSource<uint> _started = new();
    private uint _threadId;

    public StaThread(string name)
    {
        _thread = new Thread(Run) { IsBackground = true, Name = name };
        _thread.SetApartmentState(ApartmentState.STA);
        _thread.Start();
        _threadId = _started.Task.GetAwaiter().GetResult();
    }

    private unsafe void Run()
    {
        OleClipboard.InitializeSta();
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
    }

    public T Invoke<T>(Func<T> f)
    {
        var tcs = new TaskCompletionSource<T>(TaskCreationOptions.RunContinuationsAsynchronously);
        _work.Add(() =>
        {
            try { tcs.SetResult(f()); } catch (Exception e) { tcs.SetException(e); }
        });
        PInvoke.PostThreadMessage(_threadId, WM_APP_WORK, default, default);
        return tcs.Task.GetAwaiter().GetResult();
    }

    public void Invoke(Action a) => Invoke(() => { a(); return 0; });

    public void Dispose()
    {
        if (_threadId == 0) return;
        PInvoke.PostThreadMessage(_threadId, PInvoke.WM_QUIT, default, default);
        _thread.Join(TimeSpan.FromSeconds(5));
        _threadId = 0;
    }
}
