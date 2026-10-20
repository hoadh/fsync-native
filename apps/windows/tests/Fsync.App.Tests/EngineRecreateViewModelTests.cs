using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Fsync.Core;

namespace Fsync.App.Tests;

/// <summary>
/// ResetHistoryAsync disposes the engine and creates a new one. The trust prompt holds the bridge, so
/// events from the new engine still reach it, and the Ready transition refreshes exactly once.
/// </summary>
public sealed class EngineRecreateViewModelTests
{
    private static int Pending(FakeEngine e) { lock (e.Calls) return e.Calls.Count(c => c == "PendingTrustResets"); }

    [Fact]
    public async Task TrustResetFromTheNewEngineReachesTheViewModel()
    {
        using var h = new Harness(new SessionTokens("a", "r", "acct"));
        await h.Bridge.InitializeAsync();
        var oldEngine = h.Engine;
        using var vm = new TrustResetPromptViewModel(h.Bridge);

        var after = new TrustResetInfo("new-reset", 1, ["Mac"]);
        var fresh = new FakeEngine();
        h.NewEngine = () => fresh;
        await h.Bridge.ResetHistoryAsync();

        Assert.Same(fresh, h.Engine);
        Assert.Equal(2, h.EnginesCreated);
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
        Assert.Contains("Stop", oldEngine.Calls);
        await BridgeTests.WaitFor(() => Pending(fresh) == 1); // the Ready transition
        await Task.Delay(50);
        Assert.Equal(1, Pending(fresh));
        Assert.False(vm.HasPending);

        fresh.OnPendingTrustResets = () => [after];
        fresh.Delegate!.OnAccountEvent(new AccountEvent.TrustReset(after.resetId, 1, ["Mac"]));
        await BridgeTests.WaitFor(() => vm.Current?.resetId == after.resetId);
        Assert.Equal(2, Pending(fresh));
        Assert.Equal(0, Pending(oldEngine));
    }
}
