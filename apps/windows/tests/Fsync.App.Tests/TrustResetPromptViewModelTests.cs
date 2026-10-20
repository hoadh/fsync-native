using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Fsync.Core;

namespace Fsync.App.Tests;

/// <summary>
/// Port of macOS TrustResetPromptModelTests on FakeEngine (the engine has nothing pending and says
/// NotFound unless a test scripts it), plus sequence, Ready-refresh and dialog-binding rules.
/// </summary>
public sealed class TrustResetPromptViewModelTests
{
    private static readonly SessionTokens Session = new("a", "r", "acct");
    private static readonly TrustResetInfo Stale = new(string.Concat(Enumerable.Repeat("ab", 32)), 1, ["Mac"]);
    private static readonly TrustResetInfo Other = new(string.Concat(Enumerable.Repeat("cd", 32)), 2, ["Mac", "Phone"]);

    /// <summary>Bridge online first, so the Ready transition's refresh is over before the test drives the VM.</summary>
    private static async Task<TrustResetPromptViewModel> Vm(Harness h)
    {
        await h.Bridge.InitializeAsync();
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
        return new TrustResetPromptViewModel(h.Bridge);
    }

    private static int CallCount(FakeEngine e, string call) { lock (e.Calls) return e.Calls.Count(c => c == call); }
    private static bool Called(FakeEngine e, string call) { lock (e.Calls) return e.Calls.Contains(call); }

    [Fact]
    public async Task RefreshOnFreshEngineIsEmpty()
    {
        using var h = new Harness(Session);
        using var vm = await Vm(h);
        vm.Apply([Stale]);
        await vm.Refresh();
        Assert.Empty(vm.Pending);
        Assert.False(vm.HasPending);
        Assert.Equal(TrustResetOutcome.Unanswered, vm.Outcome);
        Assert.Null(vm.Message);
    }

    [Fact]
    public async Task NothingPendingGoesToTheEngineAndReportsNotFound()
    {
        using var h = new Harness(Session);
        using var vm = await Vm(h);
        foreach (var confirm in new[] { true, false })
        {
            vm.Apply([Stale]);
            if (confirm)
            {
                Assert.True(vm.BeginConfirm(Stale.resetId));
                await vm.ConfirmCommand.ExecuteAsync(Stale.resetId);
            }
            else await vm.DeclineCommand.ExecuteAsync(Stale.resetId);
            Assert.Equal(TrustResetMessages.NotFound.Text, vm.Message);
            Assert.True(vm.MessageIsNotice);
            Assert.False(vm.IsBusy);
            Assert.Equal(TrustResetOutcome.Unanswered, vm.Outcome);
        }
        Assert.True(Called(h.Engine, $"ConfirmTrustReset {Stale.resetId}"));
        Assert.True(Called(h.Engine, $"DeclineTrustReset {Stale.resetId}"));
    }

    /// <summary>The list changes during the call: nothing is trusted, the id the user saw is the one sent.</summary>
    [Fact]
    public async Task ListChangeDuringAnswerRefreshesAndTrustsNothing()
    {
        using var h = new Harness(Session);
        using var vm = await Vm(h);
        foreach (var confirm in new[] { true, false })
        {
            using var gate = new ManualResetEventSlim();
            h.Engine.OnConfirmTrustReset = _ => { gate.Wait(); throw new FsyncException.NotFound(); };
            h.Engine.OnDeclineTrustReset = _ => { gate.Wait(); throw new FsyncException.NotFound(); };
            vm.Apply([Stale]);
            Task answer;
            if (confirm)
            {
                vm.BeginConfirm(Stale.resetId);
                answer = vm.ConfirmAsync(Stale.resetId);
            }
            else answer = vm.DeclineAsync(Stale.resetId);
            vm.Apply([Other]); // the offer grew while the click was in flight
            gate.Set();
            await answer;
            Assert.Equal(TrustResetOutcome.Unanswered, vm.Outcome);
            Assert.True(vm.MessageIsNotice);
            Assert.Empty(vm.Pending); // NotFound refreshes from the engine
            Assert.False(vm.IsBusy);
        }
        Assert.True(Called(h.Engine, $"ConfirmTrustReset {Stale.resetId}"));
        Assert.True(Called(h.Engine, $"DeclineTrustReset {Stale.resetId}"));
        Assert.False(Called(h.Engine, $"ConfirmTrustReset {Other.resetId}"));
    }

    [Fact]
    public async Task DoubleClickIsIgnoredWhileBusy()
    {
        using var h = new Harness(Session);
        using var vm = await Vm(h);
        using var gate = new ManualResetEventSlim();
        h.Engine.OnConfirmTrustReset = _ => { gate.Wait(); throw new FsyncException.NotFound(); };
        vm.Apply([Stale]);
        vm.BeginConfirm(Stale.resetId);
        var first = vm.ConfirmAsync(Stale.resetId);
        Assert.True(vm.IsBusy);
        await vm.DeclineAsync(Stale.resetId);
        Assert.False(vm.BeginConfirm(Stale.resetId)); // no dialog while busy
        gate.Set();
        await first;
        Assert.False(vm.IsBusy);
        Assert.False(Called(h.Engine, $"DeclineTrustReset {Stale.resetId}"));
    }

    [Fact]
    public void ChangedFirstIdClearsMessageAndNoticesReplacement()
    {
        using var h = new Harness();
        using var vm = new TrustResetPromptViewModel(h.Bridge);
        vm.Apply([Stale]);
        vm.Apply([Stale]); // same id: nothing happens
        Assert.Null(vm.Message);
        vm.Apply([Other]); // replaced while shown
        Assert.Equal(TrustResetMessages.ListChanged.Text, vm.Message);
        Assert.True(vm.MessageIsNotice);
        vm.Apply([]); // gone: message cleared, no notice
        Assert.Null(vm.Message);
        Assert.False(vm.MessageIsNotice);
    }

    [Fact]
    public void DismissOutcomeClearsMessage()
    {
        using var h = new Harness();
        using var vm = new TrustResetPromptViewModel(h.Bridge);
        vm.Apply([Stale]);
        vm.Apply([new TrustResetInfo("x", 1, ["Mac"])]);
        Assert.NotNull(vm.Message);
        vm.DismissOutcome();
        Assert.Null(vm.Message);
        Assert.Equal(TrustResetOutcome.Unanswered, vm.Outcome);
    }

    [Fact]
    public async Task RefreshesOnlyOnTrustReset()
    {
        using var h = new Harness(Session);
        using var vm = await Vm(h);
        vm.Apply([Stale]);
        AccountEvent[] others = [new AccountEvent.ListAdvanced(1), new AccountEvent.HistoryReset("b"), new AccountEvent.SelfRemoved(), new AccountEvent.SessionExpired()];
        foreach (var ev in others) h.Engine.Delegate!.OnAccountEvent(ev);
        await Task.Delay(50);
        Assert.Equal(0, CallCount(h.Engine, "PendingTrustResets"));
        Assert.Equal([Stale], vm.Pending);
        h.Engine.Delegate!.OnAccountEvent(new AccountEvent.TrustReset(Stale.resetId, 1, ["Mac"]));
        await BridgeTests.WaitFor(() => !vm.HasPending); // TrustReset refreshes from the engine
    }

    [Fact]
    public async Task OlderRefreshResultIsDropped()
    {
        using var h = new Harness(Session);
        using var vm = await Vm(h);
        using var gate = new ManualResetEventSlim();
        var calls = 0;
        h.Engine.OnPendingTrustResets = () =>
        {
            if (Interlocked.Increment(ref calls) == 1) { gate.Wait(); return [Stale]; }
            return [Other];
        };
        var older = vm.Refresh();
        await BridgeTests.WaitFor(() => Volatile.Read(ref calls) == 1);
        await vm.Refresh();
        Assert.Equal(Other.resetId, vm.Current?.resetId);
        gate.Set();
        await older;
        Assert.Equal(Other.resetId, vm.Current?.resetId);
    }

    [Fact]
    public async Task ReadyTransitionRefreshes()
    {
        using var h = new Harness(Session);
        h.Engine.OnPendingTrustResets = () => [Stale];
        using var vm = new TrustResetPromptViewModel(h.Bridge);
        await h.Bridge.InitializeAsync();
        await BridgeTests.WaitFor(() => vm.HasPending);
        Assert.Equal(1, CallCount(h.Engine, "PendingTrustResets"));
        Assert.Equal(["Mac"], vm.DisplayNames);
    }

    [Fact]
    public void DialogClosesWhenTheShownResetChanges()
    {
        using var h = new Harness();
        using var vm = new TrustResetPromptViewModel(h.Bridge);
        var aborted = 0;
        vm.ConfirmAborted += () => aborted++;
        vm.Apply([Stale]);
        Assert.True(vm.BeginConfirm(Stale.resetId));
        vm.Apply([Stale, Other]); // same shown id: dialog stays
        Assert.Equal(Stale.resetId, vm.ConfirmingResetId);
        vm.Apply([Other]);
        Assert.Null(vm.ConfirmingResetId);
        Assert.Equal(1, aborted);
        Assert.Equal(TrustResetMessages.ListChanged.Text, vm.Message);
    }

    [Fact]
    public async Task ConfirmWithAnIdOtherThanTheDialogsIsANoOp()
    {
        using var h = new Harness(Session);
        h.Engine.OnConfirmTrustReset = _ => AccountState.Ready;
        using var vm = await Vm(h);
        vm.Apply([Stale]);
        Assert.False(vm.BeginConfirm(Other.resetId)); // not the shown reset
        await vm.ConfirmAsync(Stale.resetId); // no dialog open
        Assert.True(vm.BeginConfirm(Stale.resetId));
        await vm.ConfirmAsync(Other.resetId); // not the dialog's id
        Assert.Equal(Stale.resetId, vm.ConfirmingResetId);
        lock (h.Engine.Calls) Assert.DoesNotContain(h.Engine.Calls, c => c.StartsWith("ConfirmTrustReset", StringComparison.Ordinal));
        vm.CancelConfirm();
        Assert.Null(vm.ConfirmingResetId);
    }

    [Theory]
    [InlineData(AccountState.Ready, TrustResetOutcome.Trusted, AccountPhase.Ready)]
    [InlineData(AccountState.Created, TrustResetOutcome.Trusted, AccountPhase.Ready)]
    [InlineData(AccountState.NeedsPairing, TrustResetOutcome.RemovedNeedsPairing, AccountPhase.NeedsPairing)]
    public async Task ConfirmMapsTheStateAndAppliesItToTheBridge(AccountState state, TrustResetOutcome outcome, AccountPhase phase)
    {
        using var h = new Harness(Session);
        h.Engine.OnConfirmTrustReset = _ => state;
        using var vm = await Vm(h);
        vm.Apply([Stale]);
        vm.BeginConfirm(Stale.resetId);
        await vm.ConfirmCommand.ExecuteAsync(Stale.resetId);
        Assert.Equal(outcome, vm.Outcome);
        Assert.Equal(phase, h.Bridge.Phase);
        Assert.Null(vm.ConfirmingResetId);
        Assert.Null(vm.Message);
        Assert.Empty(vm.Pending);
    }

    [Fact]
    public async Task DeclineKeepsTheCurrentDevices()
    {
        using var h = new Harness(Session);
        h.Engine.OnDeclineTrustReset = _ => { };
        using var vm = await Vm(h);
        vm.Apply([Stale]);
        await vm.DeclineCommand.ExecuteAsync(Stale.resetId);
        Assert.Equal(TrustResetOutcome.Declined, vm.Outcome);
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
    }

    [Fact]
    public async Task FailureForTheShownResetIsReported()
    {
        using var h = new Harness(Session);
        h.Engine.OnDeclineTrustReset = _ => throw new FsyncException.Network("down");
        using var vm = await Vm(h);
        vm.Apply([Stale]);
        await vm.DeclineAsync(Stale.resetId);
        Assert.Equal(TrustResetMessages.Network.Text, vm.Message);
        Assert.False(vm.MessageIsNotice);
        Assert.DoesNotContain("down", vm.Message); // details are never shown
        Assert.Equal(TrustResetOutcome.Unanswered, vm.Outcome);
    }

    [Fact]
    public void MessagesMapErrors()
    {
        Assert.Equal(TrustResetMessages.SignIn, TrustResetMessages.For(new FsyncException.Auth()));
        Assert.Equal(TrustResetMessages.SignIn, TrustResetMessages.For(new FsyncException.ReauthRequired()));
        Assert.Equal(TrustResetMessages.Other, TrustResetMessages.For(new FsyncException.Internal("x")));
        Assert.Equal("Please sign in again, then retry.", TrustResetMessages.SignIn.Text);
        Assert.Equal("Something went wrong. Please try again.", TrustResetMessages.Other.Text);
    }

    [Fact]
    public void DisplayNamesAreSanitized()
    {
        using var h = new Harness();
        using var vm = new TrustResetPromptViewModel(h.Bridge);
        vm.Apply([new TrustResetInfo("x", 2, ["Mac\n mini", "‮​"])]);
        Assert.Equal(["Mac mini", "another device"], vm.DisplayNames);
    }

    [Fact]
    public async Task RefreshFinishingAfterDisposeChangesNothing()
    {
        using var h = new Harness(Session);
        var vm = await Vm(h);
        vm.Apply([Stale]);
        vm.BeginConfirm(Stale.resetId);
        var aborted = 0;
        vm.ConfirmAborted += () => aborted++;
        using var gate = new ManualResetEventSlim();
        h.Engine.OnPendingTrustResets = () => { gate.Wait(); return [Other]; };
        var refresh = vm.Refresh();
        await BridgeTests.WaitFor(() => CallCount(h.Engine, "PendingTrustResets") == 1);
        vm.Dispose();
        gate.Set();
        await refresh;
        Assert.Equal(Stale.resetId, vm.Current?.resetId);
        Assert.Equal(Stale.resetId, vm.ConfirmingResetId);
        Assert.Equal(0, aborted);
        Assert.Null(vm.Message);
        await vm.Refresh(); // after Dispose: no engine call
        Assert.Equal(1, CallCount(h.Engine, "PendingTrustResets"));
    }

    [Fact]
    public async Task DeclineClosesAnOpenTrustDialogForThatReset()
    {
        using var h = new Harness(Session);
        h.Engine.OnDeclineTrustReset = _ => { };
        h.Engine.OnPendingTrustResets = () => [Stale]; // still shown after the first answer's refresh
        using var vm = await Vm(h);
        vm.Apply([Stale]);
        Assert.True(vm.BeginConfirm(Stale.resetId));
        await vm.DeclineAsync(Other.resetId); // another id: the dialog stays
        Assert.Equal(Stale.resetId, vm.ConfirmingResetId);
        await vm.DeclineAsync(Stale.resetId);
        Assert.Null(vm.ConfirmingResetId);
        Assert.Equal(TrustResetOutcome.Declined, vm.Outcome);
    }

    [Fact]
    public async Task DisposeUnsubscribes()
    {
        using var h = new Harness(Session);
        var vm = await Vm(h);
        vm.Dispose();
        h.Engine.Delegate!.OnAccountEvent(new AccountEvent.TrustReset("x", 1, ["Mac"]));
        h.Engine.OnEnsure = () => AccountState.NeedsPairing;
        await h.Bridge.GoOnlineAsync();
        h.Engine.OnEnsure = () => AccountState.Ready;
        await h.Bridge.GoOnlineAsync(); // a Ready transition
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
        await Task.Delay(50);
        Assert.Equal(0, CallCount(h.Engine, "PendingTrustResets"));
    }
}
