namespace Fsync.App.Bridge;

/// <summary>Runs work on the UI thread (WinUI: DispatcherQueue.TryEnqueue; tests: inline).</summary>
public interface IUiDispatcher
{
    void Post(Action action);
}

/// <summary>Runs posted work immediately on the calling thread (tests, headless use).</summary>
public sealed class InlineDispatcher : IUiDispatcher
{
    public void Post(Action action) => action();
}
