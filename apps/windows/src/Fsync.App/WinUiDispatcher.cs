using Fsync.App.Bridge;
using Microsoft.UI.Dispatching;

namespace Fsync.App;

/// <summary>Core callbacks (Rust thread) -> UI thread.</summary>
internal sealed class WinUiDispatcher(DispatcherQueue queue) : IUiDispatcher
{
    public void Post(Action action)
    {
        if (!queue.TryEnqueue(() => action()))
            System.Diagnostics.Debug.WriteLine("fsync: UI queue closed, event dropped");
    }
}
