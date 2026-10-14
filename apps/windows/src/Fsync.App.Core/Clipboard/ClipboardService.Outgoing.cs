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
            var gen = Interlocked.Increment(ref _outGen);
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
        var gen = Interlocked.Increment(ref _outGen);
        var snap = ReadSnapshot();
        if (snap.Skip == SkipReason.None) Observe(PublishAsync(snap, gen), "publish");
        else AppLog.Info($"clipboard: change skipped: {snap.Describe()}");
    }

    private async Task PublishAsync(ClipboardSnapshot snap, long gen)
    {
        await _outLane.WaitAsync().ConfigureAwait(false);
        try
        {
            if (gen != Volatile.Read(ref _outGen))
            {
                AppLog.Info("clipboard: change superseded before publishing");
                return;
            }
            var clip = await RepresentationMapper.ToLocalClipAsync(snap);
            if (clip is null) return;
            var r = await _gate.RunAsync(() => _bridge.RunAsync(e => e.PublishClip(clip)));
            AppLog.Info($"clipboard: published reps={clip.reps.Length} files={clip.files.Length} echo={r.echo} delivered={r.delivered.Length} failed={r.failed.Length}");
            if (r.echo) return; // the clip we just received: core's echo guard
            Post(() =>
            {
                LastActivity = r.delivered.Length == 0 ? "Copied, but no other device is connected"
                    : $"Sent to {r.delivered.Length} device{(r.delivered.Length == 1 ? "" : "s")}";
                LastError = r.failed.Length > 0 ? $"{r.failed.Length} device(s) did not get it" : null;
            });
        }
        catch (OperationCanceledException) { } // quitting
        catch (Fsync.Core.FsyncException e)
        {
            AppLog.Warn($"clipboard: publish failed: {e.GetType().Name}");
            Post(() => LastError = UserMessages.For(e));
        }
        catch (Exception e)
        {
            AppLog.Warn($"clipboard: publish failed: {e.GetType().Name}");
            Post(() => LastError = "Could not send the clipboard.");
        }
        finally { _outLane.Release(); }
    }
}
