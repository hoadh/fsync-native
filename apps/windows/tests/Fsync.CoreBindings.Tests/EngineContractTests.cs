using System.Diagnostics;
using Fsync.Core;

namespace Fsync.CoreBindings.Tests;

/// <summary>C# port of FsyncCoreKitTests.swift (contract v2), plus Windows-side checks.</summary>
public sealed class EngineContractTests : EngineTestBase
{
    [Fact]
    public void ContractVersion() => Assert.Equal(2u, FsyncFfi.ContractVersion());

    [Fact]
    public void SettingsHistoryDevicesOnFreshEngine()
    {
        using var engine = new FsyncEngine(Config(TempDir(), 9), new Recorder());
        Assert.Equal(64, engine.NodeId().Length);
        Assert.Empty(engine.Devices());
        Assert.Empty(engine.History(null, null, 20).items);

        var s = engine.Settings();
        Assert.Equal(ClipMode.Auto, s.clipMode);
        engine.SetSettings(s with { autoAcceptFiles = true, denylist = ["com.example.secrets"] });
        var back = engine.Settings();
        Assert.Equal(["com.example.secrets"], back.denylist);
        Assert.True(back.autoAcceptFiles);
    }

    [Fact]
    public void ErrorsAreTyped()
    {
        using var engine = new FsyncEngine(Config(TempDir(), 9), new Recorder());
        Assert.Throws<FsyncException.Auth>(() => engine.EnsureAccount());
        Assert.Throws<FsyncException.Invalid>(() => engine.History(null, "garbage", 5));
        var bad = Config(TempDir(), 9) with { historyKey = Filled(1, 5) };
        Assert.Throws<FsyncException.Invalid>(() => new FsyncEngine(bad, new Recorder()));
    }

    /// Nothing pending, unknown ids are NotFound (also when malformed), reset needs a session.
    [Fact]
    public void TrustResetApiOnAFreshEngine()
    {
        using var engine = new FsyncEngine(Config(TempDir(), 9), new Recorder());
        Assert.Empty(engine.PendingTrustResets());
        foreach (var id in new[] { "bogus", string.Concat(Enumerable.Repeat("ab", 32)) })
        {
            Assert.Throws<FsyncException.NotFound>(() => engine.ConfirmTrustReset(id));
            Assert.Throws<FsyncException.NotFound>(() => engine.DeclineTrustReset(id));
        }
        Assert.Throws<FsyncException.Auth>(() => engine.ResetAccountTrust());
    }

    [Fact]
    public void TrustResetPromptCarriesDeviceNames()
    {
        var info = new TrustResetInfo(string.Concat(Enumerable.Repeat("ab", 32)), 2, ["Mac", "iPhone"]);
        Assert.Equal((int)info.devices, info.deviceNames.Length);
        AccountEvent ev = new AccountEvent.TrustReset(info.resetId, info.devices, info.deviceNames);
        var tr = Assert.IsType<AccountEvent.TrustReset>(ev);
        Assert.Equal(info.resetId, tr.resetId);
        Assert.Equal(2u, tr.devices);
        Assert.Equal(["Mac", "iPhone"], tr.deviceNames);
    }

    [Fact]
    public void DelegateCallbackArrivesOffTheCallingThread()
    {
        var dir = TempDir();
        new FsyncEngine(Config(dir, 9), new Recorder()).Dispose();
        // Same data, different history key: a distinct error, nothing is moved aside.
        Assert.Throws<FsyncException.HistoryKeyMismatch>(() => new FsyncEngine(Config(dir, 10), new Recorder()));
        // A genuinely damaged file is reset and the app is told, off the calling thread.
        File.WriteAllBytes(Path.Combine(dir, "history.db"), Filled(0x5A, 20_000));
        File.Delete(Path.Combine(dir, "history.db-wal"));
        var rec = new Recorder();
        using var engine = new FsyncEngine(Config(dir, 9), rec);
        Assert.True(rec.GotAccountEvent.Wait(TimeSpan.FromSeconds(10)), "no account event");
        var reset = Assert.IsType<AccountEvent.HistoryReset>(rec.Events.First());
        Assert.False(string.IsNullOrEmpty(reset.backup));
        Assert.False(rec.OnTestThread);
    }

    /// The user confirms, the old history is moved aside, the engine opens with the new key.
    [Fact]
    public void ResetHistoryRecoversFromAKeyMismatch()
    {
        var dir = TempDir();
        new FsyncEngine(Config(dir, 9), new Recorder()).Dispose();
        Assert.Throws<FsyncException.HistoryKeyMismatch>(() => new FsyncEngine(Config(dir, 10), new Recorder()));
        FsyncFfi.ResetHistory(dir);
        using var engine = new FsyncEngine(Config(dir, 10), new Recorder());
        Assert.Contains(Directory.EnumerateFileSystemEntries(dir),
            p => Path.GetFileName(p).StartsWith("history.corrupt-", StringComparison.Ordinal));
    }

    /// 10 MiB crosses into Rust (as a config field) and comes back as a typed error, not a crash.
    [Fact]
    public void LargeDataCrossesTheBoundary()
    {
        var cfg = Config(TempDir(), 9) with { historyKey = Filled(0xA5, 10 * 1024 * 1024) };
        var sw = Stopwatch.StartNew();
        Assert.Throws<FsyncException.Invalid>(() => new FsyncEngine(cfg, new Recorder()));
        Assert.True(sw.ElapsedMilliseconds < 2000, $"{sw.ElapsedMilliseconds} ms");
    }

    /// Network-dependent calls on an engine that was never started or signed in: typed errors only.
    [Fact]
    public void UnstartedEngineFailsWithTypedErrors()
    {
        using var engine = new FsyncEngine(Config(TempDir(), 9), new Recorder());
        var clip = new LocalClip([new Rep("text/plain;charset=utf-8", "hi"u8.ToArray())], [], "hi", false);
        Assert.ThrowsAny<FsyncException>(() => engine.PublishClip(clip));
        Assert.ThrowsAny<FsyncException>(() => engine.FetchRep(Guid.NewGuid().ToString(), 0));
        Assert.ThrowsAny<FsyncException>(() => engine.ReadFileRange(Guid.NewGuid().ToString(), 0, 0, 1024));
        Assert.ThrowsAny<FsyncException>(() => engine.PairingOffer(PairingMode.Code));
        Assert.Throws<FsyncException.NotFound>(() => engine.Pin(Guid.NewGuid().ToString(), true));
        engine.Clear(keepPinned: false);
    }
}
