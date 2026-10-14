using Fsync.App.Clipboard;
using Fsync.App.Files;
using Fsync.App.ViewModels;
using Fsync.Core;

namespace Fsync.App.Tests;

/// <summary>Phase 14 view models against the real engine (settings, history, transfers).</summary>
[Collection("clipboard")] // ClipboardService owns an STA clipboard thread; keep it off parallel runs
public sealed class RealEngineViewModelTests
{
    private static ClipSettingsViewModel Settings(RealEngine r) =>
        new(r.Bridge, r.Clipboard, r.Prefs, new SendToShortcut(@"C:\Apps\Fsync.exe", Path.Combine(r.Dir, "sendto")));

    [Fact]
    public async Task ClipSettingsAreStoredByCoreAndReachTheClipboardService()
    {
        using var r = await RealEngine.StartAsync();
        var vm = Settings(r);
        await vm.LoadAsync();
        Assert.Null(vm.Error);
        Assert.False(vm.ManualMode);

        vm.ManualMode = true;
        vm.AutoAcceptFiles = true;
        vm.RetentionDays = 7;
        vm.MaxClipMb = 10;
        vm.Denylist = "KeePass\r\nkeepass\r\n  Bitwarden  \r\n";
        await vm.SaveAsync();
        Assert.Null(vm.Error);
        Assert.True(r.Clipboard.IsManual);
        Assert.True(r.Clipboard.CurrentSettings!.autoAcceptFiles);

        var again = Settings(r);
        await again.LoadAsync();
        Assert.True(again.ManualMode);
        Assert.Equal(7, again.RetentionDays);
        Assert.Equal(10, again.MaxClipMb);
        Assert.Equal("KeePass" + Environment.NewLine + "Bitwarden", again.Denylist);
        var stored = await r.Bridge.RunAsync(e => e.Settings());
        Assert.Equal(10UL * 1024 * 1024, stored.maxClipBytes);
    }

    [Fact]
    public async Task AppPreferencesPersistAndBrokenFilesFallBackToDefaults()
    {
        using var r = await RealEngine.StartAsync();
        var vm = Settings(r);
        vm.EagerFiles = true;
        vm.Hotkeys = false;
        var loaded = Services.AppPreferences.Load(r.Settings.PreferencesPath);
        Assert.Equal(FileClipStrategy.Eager, loaded.FileStrategy);
        Assert.False(loaded.HotkeysEnabled);

        File.WriteAllText(r.Settings.PreferencesPath, "{ not json");
        Assert.Equal(FileClipStrategy.Lazy, Services.AppPreferences.Load(r.Settings.PreferencesPath).FileStrategy);
    }

    [Fact]
    public async Task SendToMenuToggleCreatesARealShortcut()
    {
        using var r = await RealEngine.StartAsync();
        var vm = Settings(r);
        var link = new SendToShortcut(@"C:\Apps\Fsync.exe", Path.Combine(r.Dir, "sendto"));
        vm.SendToMenu = true;
        Assert.Null(vm.Error);
        Assert.True(link.IsInstalled);
        Assert.Equal((@"C:\Apps\Fsync.exe", "--send"), Interop.Shell.ShellLink.Read(link.LinkPath));
        vm.SendToMenu = false;
        Assert.False(File.Exists(link.LinkPath));
    }

    [Fact]
    public async Task EmptyHistorySearchAndClear()
    {
        using var r = await RealEngine.StartAsync();
        var vm = new HistoryViewModel(r.Bridge, r.Clipboard);
        await vm.RefreshAsync();
        Assert.Null(vm.Error);
        Assert.Empty(vm.Items);
        Assert.False(vm.HasMore);
        Assert.Equal("Nothing copied yet.", vm.Status);

        vm.Query = "needle";
        await vm.RefreshAsync();
        Assert.Equal("No matches.", vm.Status);
        await vm.ClearAsync(keepPinned: false);
        Assert.Null(vm.Error);
    }

    [Fact]
    public async Task UnknownItemsCannotBeCopiedBackOrPinned()
    {
        using var r = await RealEngine.StartAsync();
        Assert.False(r.Clipboard.CanCopyBack(Guid.NewGuid().ToString()));
        Assert.False(await r.Clipboard.CopyBackAsync(Guid.NewGuid().ToString()));
        var vm = new HistoryViewModel(r.Bridge, r.Clipboard);
        var ghost = new HistoryRow(new HistoryItem(Guid.NewGuid().ToString(), true, null, ClipKind.Text, 1, 1, "x", false), "", false);
        await vm.TogglePinAsync(ghost);
        Assert.Equal("It no longer exists.", vm.Error); // core: NotFound
    }

    [Fact]
    public async Task SendingToAnUnpairedDeviceFailsWithAClearMessage()
    {
        using var r = await RealEngine.StartAsync();
        using var vm = new TransfersViewModel(r.Bridge, () => Path.Combine(r.Dir, "dl"), () => false);
        var file = Path.Combine(r.Dir, "a.txt");
        File.WriteAllText(file, "hello");
        Assert.False(await vm.SendAsync(new string('c', 64), "Ghost", [file]));
        Assert.NotNull(vm.Error);
        Assert.Empty(vm.Transfers);
    }
}
