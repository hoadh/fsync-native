using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Fsync.Core;

namespace Fsync.App.Tests;

public sealed class ViewModelTests
{
    [Fact]
    public async Task OnboardingSendsCodeThenVerifies()
    {
        using var h = new Harness();
        await h.Bridge.InitializeAsync();
        var vm = new OnboardingViewModel(h.Bridge) { Email = " me@example.com " };
        Assert.True(vm.SendCodeCommand.CanExecute(null));
        await vm.SendCodeCommand.ExecuteAsync(null);
        Assert.Equal(OnboardingStep.Code, vm.Step);
        Assert.Contains("LoginStart me@example.com", h.Engine.Calls);

        h.Engine.OnLoginVerify = (_, _) => throw new FsyncException.Auth();
        vm.Code = "000000";
        await vm.VerifyCommand.ExecuteAsync(null);
        Assert.Equal("That code is wrong or has expired.", vm.Error);
        Assert.Equal(AccountPhase.SignedOut, h.Bridge.Phase);

        h.Engine.OnLoginVerify = (_, _) => new SessionTokens("a", "r", "acct");
        vm.Code = "123456";
        await vm.VerifyCommand.ExecuteAsync(null);
        Assert.Null(vm.Error);
        Assert.Equal(AccountPhase.Ready, h.Bridge.Phase);
    }

    [Theory]
    [InlineData("me@example.com", true)]
    [InlineData("me@example", false)]
    [InlineData("@example.com", false)]
    [InlineData("me @example.com", false)]
    public void EmailCheck(string email, bool ok) => Assert.Equal(ok, OnboardingViewModel.IsPlausibleEmail(email));

    [Fact]
    public async Task NewDeviceOffersThenConfirmsTheSasFromTheEvent()
    {
        using var h = new Harness(new SessionTokens("a", "r", "acct"));
        h.Engine.OnEnsure = () => AccountState.NeedsPairing;
        await h.Bridge.InitializeAsync();
        using var vm = new PairingViewModel(h.Bridge);

        await vm.OfferCommand.ExecuteAsync(PairingMode.Code);
        Assert.Equal(PairingStage.Offering, vm.Stage);
        Assert.Equal("123-456", vm.OfferCode);
        Assert.Null(vm.QrPng);

        h.Engine.Delegate!.OnPairingEvent(new PairingEvent.ConfirmRequest(FakeEngine.Peer(41)));
        Assert.Equal(PairingStage.ConfirmSas, vm.Stage);
        Assert.Equal("apple banana cherry", vm.SasWords);
        Assert.Equal("Mac", vm.PeerPlatform);

        // A newer request replaces the older id.
        h.Engine.Delegate.OnPairingEvent(new PairingEvent.ConfirmRequest(FakeEngine.Peer(42)));
        await vm.AcceptCommand.ExecuteAsync(null);
        Assert.Contains("PairingConfirm 42 True", h.Engine.Calls);
        Assert.Equal(PairingStage.WaitingForOther, vm.Stage);

        h.Engine.OnEnsure = () => AccountState.Ready;
        h.Engine.Delegate.OnPairingEvent(new PairingEvent.Completed(new string('b', 64), 3));
        Assert.Equal(PairingStage.Completed, vm.Stage);
        await BridgeTests.WaitFor(() => h.Bridge.Phase == AccountPhase.Ready);
    }

    [Fact]
    public async Task QrOfferRendersAPng()
    {
        using var h = new Harness(new SessionTokens("a", "r", "acct"));
        h.Engine.OnOffer = _ => new PairingOffer("fsync://pair?c=abc&n=xyz", null, 4_102_444_800_000);
        await h.Bridge.InitializeAsync();
        using var vm = new PairingViewModel(h.Bridge);
        await vm.OfferCommand.ExecuteAsync(PairingMode.Qr);
        Assert.NotNull(vm.QrPng);
        Assert.Equal([0x89, (byte)'P', (byte)'N', (byte)'G'], vm.QrPng![..4]);
    }

    [Fact]
    public async Task ExistingDeviceJoinsAndCompletionWinsOverTheConfirmReturn()
    {
        using var h = new Harness(new SessionTokens("a", "r", "acct"));
        await h.Bridge.InitializeAsync();
        using var vm = new PairingViewModel(h.Bridge) { JoinInput = " 123-456 " };
        await vm.JoinCommand.ExecuteAsync(null);
        Assert.Equal(PairingStage.ConfirmSas, vm.Stage);
        Assert.Contains("PairingJoin 123-456", h.Engine.Calls);

        // Completed arrives while pairing_confirm is still running.
        h.Engine.OnConfirm = (_, _) => h.Engine.Delegate!.OnPairingEvent(new PairingEvent.Completed("x", 1));
        await vm.AcceptCommand.ExecuteAsync(null);
        Assert.Equal(PairingStage.Completed, vm.Stage);
    }

    [Fact]
    public async Task RejectedByTheOtherSideIsExplained()
    {
        using var h = new Harness(new SessionTokens("a", "r", "acct"));
        h.Engine.OnJoin = _ => throw new FsyncException.Cancelled();
        await h.Bridge.InitializeAsync();
        using var vm = new PairingViewModel(h.Bridge) { JoinInput = "123-456" };
        await vm.JoinCommand.ExecuteAsync(null);
        Assert.Equal(PairingStage.Failed, vm.Stage);
        Assert.Equal("The other device rejected the pairing.", vm.Error);
    }

    [Fact]
    public async Task DevicesRefreshUpdateAndRemove()
    {
        using var h = new Harness(new SessionTokens("a", "r", "acct"));
        h.Engine.OnDevices = () => [FakeEngine.Dev("b", "zeta", DeviceStatus.Offline), FakeEngine.Dev("a", "this", DeviceStatus.Offline, self: true)];
        await h.Bridge.InitializeAsync();
        using var vm = new DevicesViewModel(h.Bridge);
        await vm.RefreshCommand.ExecuteAsync(null);
        Assert.Equal(["this", "zeta"], vm.Devices.Select(d => d.Name));
        Assert.Equal(0, vm.ConnectedPeers);

        h.Engine.Delegate!.OnDeviceStatus(FakeEngine.Dev("b", "zeta", DeviceStatus.Lan, online: true));
        Assert.Equal(StatusKind.Good, vm.Devices[1].Kind);
        Assert.Equal(1, vm.ConnectedPeers);

        Assert.False(vm.Devices[0].CanRemove);
        await vm.RemoveCommand.ExecuteAsync(vm.Devices[1]);
        Assert.Contains($"RemoveDevice {"b".PadRight(64, '0')}", h.Engine.Calls);
    }
}
