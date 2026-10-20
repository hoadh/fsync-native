using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Fsync.Core;

namespace Fsync.App.Tests;

/// <summary>Port of macOS AccountRecoveryModelTests on FakeEngine, plus the bridge mapping (EngineBridge.Trust).</summary>
public sealed class AccountRecoveryViewModelTests
{
    private static readonly SessionTokens Session = new("a", "r", "acct");

    private static async Task<AccountRecoveryViewModel> Vm(Harness h, AccountState start = AccountState.NeedsPairing)
    {
        h.Engine.OnEnsure = () => start;
        await h.Bridge.InitializeAsync();
        return new AccountRecoveryViewModel(h.Bridge);
    }

    private static bool ResetCalled(FakeEngine e) { lock (e.Calls) return e.Calls.Contains("ResetAccountTrust"); }

    [Fact]
    public async Task DestructiveCallNeedsBothSteps()
    {
        using var h = new Harness(Session);
        var vm = await Vm(h);
        await vm.ResetAsync(); // idle
        vm.Begin();
        Assert.Equal(RecoveryStep.Explain, vm.Step);
        Assert.True(vm.IsPresented);
        await vm.ResetAsync(); // explain only
        vm.Cancel();
        Assert.Equal(RecoveryStep.Idle, vm.Step);
        Assert.False(vm.IsPresented);
        vm.Proceed(); // ignored from idle
        Assert.Equal(RecoveryStep.Idle, vm.Step);
        Assert.False(ResetCalled(h.Engine));
        Assert.Equal(AccountPhase.NeedsPairing, h.Bridge.Phase);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task SignInErrorsRouteToSignIn(bool reauth)
    {
        using var h = new Harness(Session);
        h.Engine.OnResetAccountTrust = () => throw (reauth ? new FsyncException.ReauthRequired() : new FsyncException.Auth());
        var vm = await Vm(h);
        vm.Begin();
        vm.Proceed();
        Assert.Equal(RecoveryStep.FinalConfirm, vm.Step);
        await vm.ResetAsync();
        Assert.True(ResetCalled(h.Engine));
        Assert.Equal(RecoveryStep.Idle, vm.Step);
        Assert.False(vm.IsBusy);
        Assert.Null(vm.Error);
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
        Assert.Equal(EngineBridge.SignInAgainMessage, h.Bridge.StatusMessage);
        Assert.Null(h.Secrets.Session);
    }

    [Fact]
    public async Task OfflineShowsErrorAndStaysOnFinalStep()
    {
        using var h = new Harness(Session);
        h.Engine.OnResetAccountTrust = () => throw new FsyncException.Network("offline");
        var vm = await Vm(h);
        vm.Begin();
        vm.Proceed();
        await vm.ResetAsync();
        Assert.NotNull(vm.Error);
        Assert.Equal(RecoveryStep.FinalConfirm, vm.Step);
        Assert.False(vm.IsBusy);
        Assert.Equal(AccountPhase.NeedsPairing, h.Bridge.Phase);
        vm.Cancel();
        Assert.Null(vm.Error);
        Assert.Equal(RecoveryStep.Idle, vm.Step);
    }

    [Fact]
    public async Task ResultUsesTheBridgeMapping()
    {
        using var h = new Harness(Session);
        var vm = await Vm(h);
        h.Bridge.ApplyAccountState(AccountState.Ready);
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
        h.Bridge.ApplyAccountState(AccountState.NeedsPairing);
        Assert.Equal(AccountPhase.NeedsPairing, h.Bridge.Phase);

        // Created is what a successful reset returns: this PC becomes the first device.
        h.Engine.OnResetAccountTrust = () => AccountState.Created;
        var devicesChanged = 0;
        h.Bridge.DevicesChanged += () => devicesChanged++;
        vm.Begin();
        vm.Proceed();
        await vm.ResetAsync();
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
        Assert.Equal(RecoveryStep.Idle, vm.Step);
        Assert.Null(vm.Error);
        Assert.Null(h.Bridge.StatusMessage);
        Assert.Equal(1, devicesChanged);
    }

    [Fact]
    public async Task AccountStateIsIgnoredOutsideReadyAndNeedsPairing()
    {
        using var signedOut = new Harness(); // no session
        await signedOut.Bridge.InitializeAsync();
        Assert.False(signedOut.Bridge.ApplyAccountState(AccountState.Ready));
        Assert.Equal(AccountPhase.SignedOut, signedOut.Bridge.Phase);

        using var offline = new Harness(Session);
        offline.Engine.OnEnsure = () => throw new FsyncException.Network("down");
        await offline.Bridge.InitializeAsync();
        Assert.False(offline.Bridge.ApplyAccountState(AccountState.NeedsPairing));
        Assert.Equal(AccountPhase.Offline, offline.Bridge.Phase);
        Assert.NotNull(offline.Bridge.StatusMessage);
    }

    /// <summary>The session expires while the reset runs: the late result does not bring the account back.</summary>
    [Fact]
    public async Task LateResetResultDoesNotOverwriteSignedOut()
    {
        using var h = new Harness(Session);
        using var gate = new ManualResetEventSlim();
        h.Engine.OnResetAccountTrust = () => { gate.Wait(); return AccountState.Created; };
        var vm = await Vm(h);
        vm.Begin();
        vm.Proceed();
        var reset = vm.ResetAsync();
        h.Engine.Delegate!.OnAccountEvent(new AccountEvent.SessionExpired());
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
        gate.Set();
        await reset;
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);
        Assert.Equal(RecoveryStep.Idle, vm.Step);
    }

    [Fact]
    public async Task BusyIgnoresStepChanges()
    {
        using var h = new Harness(Session);
        using var gate = new ManualResetEventSlim();
        h.Engine.OnResetAccountTrust = () => { gate.Wait(); return AccountState.Ready; };
        var vm = await Vm(h);
        vm.Begin();
        vm.Proceed();
        var reset = vm.ResetAsync();
        Assert.True(vm.IsBusy);
        vm.Cancel();
        vm.Begin();
        await vm.ResetAsync(); // second call while busy
        Assert.Equal(RecoveryStep.FinalConfirm, vm.Step);
        gate.Set();
        await reset;
        Assert.Equal(RecoveryStep.Idle, vm.Step);
        lock (h.Engine.Calls) Assert.Single(h.Engine.Calls, c => c == "ResetAccountTrust");
    }
}
