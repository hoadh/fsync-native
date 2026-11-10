using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.Interop.Clipboard;

namespace Fsync.App.Clipboard;

/// <summary>Local changes → publish_clip: one at a time, a newer change supersedes a queued one.</summary>
public sealed partial class ClipboardService
{
    private readonly SemaphoreSlim _outLane = new(1, 1);
    private long _outGen;

    /// <summary>Publishes what is on the clipboard now, in any mode (menu, hotkey).</summary>
    public async Task SendNowAsync()
    {
        try
        {
            var gen = NextOutGeneration();
            var snap = await _sta.InvokeAsync(ReadSnapshot);
            if (snap.Skip != SkipReason.None)
            {
                Post(() => LastActivity = snap.Skip is SkipReason.NoContent ? "Nothing to send" : $"Not sent ({Describe(snap.Skip)})");
                return;
            }
            await PublishAsync(snap, gen);
        }
        catch (Exception e)
        {
            AppLog.Warn($"clipboard: send now failed: {e.GetType().Name}");
            Post(() => LastError = "Could not read the clipboard.");
        }
    }

    /// <summary>Snapshot with the built-in password-manager list always applied on top of the user's.</summary>
    private ClipboardSnapshot ReadSnapshot()
    {
        var s = _settings;
        return ClipboardReader.Read((long)(s?.maxClipBytes ?? 64 * 1024 * 1024), SensitiveFormatFilter.EffectiveDenylist(s?.denylist));
    }

    private void ReadAndPublishIfAuto()
    {
        if (IsManual || _bridge.Phase != AccountPhase.Ready || _settings is null)
        {
            AppLog.Info($"clipboard: change not published (manual {IsManual}, phase {_bridge.Phase}, settings {_settings is not null})");
            return;
        }
        var gen = NextOutGeneration();
        var snap = ReadSnapshot();
        if (snap.Skip == SkipReason.None) Observe(PublishAsync(snap, gen), "publish");
        else AppLog.Info($"clipboard: change skipped: {snap.Describe()}");
    }

    /// <summary>What happened to one publish attempt (tests, logs).</summary>
    internal enum PublishOutcome { Superseded, Nothing, Sent, Failed }

    /// <summary>Next generation of local changes: a publish of an older one is dropped.</summary>
    internal long NextOutGeneration() => Interlocked.Increment(ref _outGen);

    internal async Task<PublishOutcome> PublishAsync(ClipboardSnapshot snap, long gen)
    {
        await _outLane.WaitAsync().ConfigureAwait(false);
        try
        {
            if (gen != Volatile.Read(ref _outGen))
            {
                AppLog.Info("clipboard: change superseded before publishing");
                return PublishOutcome.Superseded;
            }
            var clip = await RepresentationMapper.ToLocalClipAsync(snap);
            if (clip is null) return PublishOutcome.Nothing;
            var r = await _gate.RunAsync(() => _bridge.RunAsync(e => e.PublishClip(clip)));
            AppLog.Info($"clipboard: published reps={clip.reps.Length} files={clip.files.Length} echo={r.echo} delivered={r.delivered.Length} failed={r.failed.Length} pending={r.pending.Length}");
            if (r.echo) return PublishOutcome.Nothing; // the clip we just received: core's echo guard
            var (activity, error) = DescribePublished(r.delivered.Length, r.pending.Length, r.failed.Length);
            Post(() =>
            {
                LastActivity = activity;
                LastError = error;
            });
            return PublishOutcome.Sent;
        }
        catch (OperationCanceledException) { return PublishOutcome.Nothing; } // quitting
        catch (Fsync.Core.FsyncException e)
        {
            AppLog.Warn($"clipboard: publish failed: {e.GetType().Name}");
            Post(() => LastError = UserMessages.For(e));
            return PublishOutcome.Failed;
        }
        catch (Exception e)
        {
            AppLog.Warn($"clipboard: publish failed: {e.GetType().Name}");
            Post(() => LastError = "Could not send the clipboard.");
            return PublishOutcome.Failed;
        }
        finally { _outLane.Release(); }
    }

    /// <summary>
    /// Status lines for one publish result. A pending device had not answered when publish_clip
    /// returned and is still being tried in the background: it is "not confirmed", never "did not
    /// get it" and never "not connected". The line is not updated when its late result comes in
    /// (core only logs it), so it never claims more than was known when it was written.
    /// </summary>
    internal static (string Activity, string? Error) DescribePublished(int delivered, int pending, int failed)
    {
        static string Devices(int n) => $"{n} device{(n == 1 ? "" : "s")}";
        var activity = delivered > 0
            ? $"Sent to {Devices(delivered)}" + (pending > 0 ? $"; {pending} not confirmed" : "")
            : pending > 0 ? $"Not confirmed by {Devices(pending)}"
            : "Copied, but no other device is connected";
        return (activity, failed > 0 ? $"{failed} device(s) did not get it" : null);
    }
}
