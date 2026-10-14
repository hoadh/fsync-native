using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.Files;

/// <summary>Pulls of accepted offers: start, failure, one automatic retry, manual Retry.</summary>
public sealed partial class TransfersViewModel
{
    private readonly Dictionary<ulong, Pull> _pulls = [];
    private readonly Dictionary<TransferItem, Pull> _failed = [];

    /// <summary>What a pull needs to start again.</summary>
    private sealed record Pull(string ItemId, uint FileId, string Name, int Attempt);

    private async Task StartPullAsync(Pull p)
    {
        var id = await _bridge.RunAsync(e => e.FetchFileTo(p.ItemId, p.FileId, _downloadDir()));
        _pulls[id] = p;
        AppLog.Info($"transfer {id}: pull started (attempt {p.Attempt})");
        Track(id, p.Name, TransferDirection.Receive);
        // fetch_file_to may return only after the dial: the outcome can already be here.
        if (Transfers.FirstOrDefault(t => t.Id == id) is { IsActive: false } row && _pulls.Remove(id) && row.State == TransferState.Failed)
            OnPullFailed(row, p, row.FailureMessage);
    }

    /// <summary>
    /// A failed pull gets a Retry button. A network failure is retried once by itself: right after
    /// the sender restarted, dials can time out for ~30 s (stale path to the same node id).
    /// </summary>
    private void OnPullFailed(TransferItem row, Pull p, string? message)
    {
        AppLog.Info($"transfer {row.Id}: pull failed (attempt {p.Attempt})");
        _failed[row] = p;
        row.CanRetry = true;
        if (p.Attempt == 0 && IsNetworkFailure(message))
        {
            row.Detail += $" (trying again in {_autoRetryDelay.TotalSeconds:0} s)";
            _ = RetryLaterAsync(row);
        }
    }

    internal static bool IsNetworkFailure(string? message) =>
        message is { } m && (m.StartsWith("network", StringComparison.OrdinalIgnoreCase) || m.Contains("timed out", StringComparison.OrdinalIgnoreCase));

    private async Task RetryLaterAsync(TransferItem row)
    {
        try
        {
            await Task.Delay(_autoRetryDelay);
            if (_failed.ContainsKey(row)) await RetryAsync(row);
        }
        catch (Exception e) { AppLog.Warn($"transfer {row.Id}: automatic retry failed: {e.GetType().Name}"); }
    }

    /// <summary>Starts a failed pull again (the old row is replaced by the new transfer).</summary>
    [RelayCommand]
    public async Task RetryAsync(TransferItem row)
    {
        if (!_failed.Remove(row, out var p)) return;
        Transfers.Remove(row);
        try
        {
            await StartPullAsync(p with { Attempt = p.Attempt + 1 });
            Error = null;
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { Error = "Could not save the file."; }
    }
}
