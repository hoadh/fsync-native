using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.App.ViewModels;
using Fsync.Core;
using Microsoft.Win32;

namespace Fsync.App.Tests;

public sealed class MappingAndStoreTests
{
    private static readonly DateTimeOffset Now = new(2026, 10, 14, 12, 0, 0, TimeSpan.Zero);

    [Theory]
    [InlineData(DeviceStatus.Lan, false, false, "Connected (same network)", StatusKind.Good)]
    [InlineData(DeviceStatus.Relay, true, false, "Connected (via relay)", StatusKind.Good)]
    [InlineData(DeviceStatus.Offline, true, false, "Online, not connected yet", StatusKind.Warning)]
    [InlineData(DeviceStatus.Offline, false, false, "Offline", StatusKind.Bad)]
    [InlineData(DeviceStatus.Lan, true, true, "This PC", StatusKind.Neutral)]
    public void DeviceStatusText(DeviceStatus status, bool online, bool self, string text, StatusKind kind)
    {
        var item = DeviceItem.From(FakeEngine.Dev("c", "x", status, online, self), Now);
        Assert.Equal(text, item.StatusText);
        Assert.Equal(kind, item.Kind);
        Assert.Equal(!self, item.CanRemove);
    }

    /// ListView rows use ToString as their accessible name: readable text, no record dump.
    [Fact]
    public void RowAccessibleName()
    {
        var item = DeviceItem.From(FakeEngine.Dev("c", "Mac mini", DeviceStatus.Lan, online: true), Now);
        Assert.Equal("Mac mini, Mac, Connected (same network)", item.ToString());
    }

    [Fact]
    public void LastSeenOnlyForOfflineDevices()
    {
        var ms = (ulong)Now.AddMinutes(-42).ToUnixTimeMilliseconds();
        Assert.Equal("Seen 42 min ago", DeviceItem.From(FakeEngine.Dev("c", "x", DeviceStatus.Offline, lastSeen: ms), Now).LastSeenText);
        Assert.Equal("", DeviceItem.From(FakeEngine.Dev("c", "x", DeviceStatus.Lan, lastSeen: ms), Now).LastSeenText);
        Assert.Equal("Never seen", DeviceItem.LastSeen(0, Now));
        Assert.Equal("Seen 3 d ago", DeviceItem.LastSeen((ulong)Now.AddDays(-3).ToUnixTimeMilliseconds(), Now));
    }

    [Theory]
    [InlineData(AccountPhase.Ready, 2, "fsync: connected to 2 devices", StatusKind.Good)]
    [InlineData(AccountPhase.Ready, 1, "fsync: connected to 1 device", StatusKind.Good)]
    [InlineData(AccountPhase.Ready, 0, "fsync: no other device connected", StatusKind.Neutral)]
    [InlineData(AccountPhase.NeedsPairing, 0, "fsync: pair this PC with one of your devices", StatusKind.Warning)]
    [InlineData(AccountPhase.SignedOut, 0, "fsync: not signed in", StatusKind.Warning)]
    public void TrayStatus(AccountPhase phase, int peers, string tooltip, StatusKind kind) =>
        Assert.Equal((tooltip, kind), TrayStatusViewModel.Compute(phase, peers, null));

    [Fact]
    public void SecretsPersistAndAreNotPlaintext()
    {
        var dir = Path.Combine(Path.GetTempPath(), "fsync-secrets-" + Guid.NewGuid().ToString("N"));
        var path = Path.Combine(dir, "secrets.dat");
        try
        {
            var a = new SecretStore(path);
            Assert.Equal(32, a.DeviceSecret.Length);
            Assert.Equal(32, a.HistoryKey.Length);
            a.SaveSession(new SessionTokens("ACCESS-TOKEN-MARKER", "r", "acct"), "me@example.com");

            var b = new SecretStore(path);
            Assert.Equal(a.DeviceSecret, b.DeviceSecret);
            Assert.Equal(a.HistoryKey, b.HistoryKey);
            Assert.Equal("ACCESS-TOKEN-MARKER", b.Session?.access);
            Assert.DoesNotContain("ACCESS-TOKEN-MARKER", File.ReadAllText(path));

            b.ClearSession();
            Assert.Null(new SecretStore(path).Session);
            Assert.Equal("me@example.com", new SecretStore(path).Email);
        }
        finally { Directory.Delete(dir, recursive: true); }
    }

    [Fact]
    public void RunAtLoginWritesAndRemovesTheValue()
    {
        var keyPath = @"Software\fsync-tests\Run-" + Guid.NewGuid().ToString("N");
        var cmd = RunAtLogin.CommandFor(@"C:\Apps\Fsync\Fsync.exe");
        try
        {
            var r = new RunAtLogin("Fsync", cmd, keyPath);
            Assert.False(r.IsEnabled);
            r.Set(true);
            Assert.True(r.IsEnabled);
            Assert.Equal("\"C:\\Apps\\Fsync\\Fsync.exe\" --background", Registry.CurrentUser.OpenSubKey(keyPath)!.GetValue("Fsync"));
            r.Set(false);
            Assert.False(r.IsEnabled);
        }
        finally { Registry.CurrentUser.DeleteSubKeyTree(@"Software\fsync-tests", throwOnMissingSubKey: false); }
    }

    [Fact]
    public void SettingsFromEnvironment()
    {
        var env = new Dictionary<string, string> { ["FSYNC_DATA_DIR"] = @"C:\tmp\fsync-b", ["FSYNC_API"] = "http://127.0.0.1:5555" };
        var s = AppSettings.FromEnvironment(k => env.GetValueOrDefault(k));
        Assert.Equal(@"C:\tmp\fsync-b\core", s.CoreDir);
        Assert.True(s.AllowInsecureHttp);
        Assert.False((s with { ApiBase = "http://api.example.com" }).AllowInsecureHttp);
        Assert.NotEqual(s.InstanceKey, (s with { DataDir = @"C:\tmp\fsync-a" }).InstanceKey);
    }
}
