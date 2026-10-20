using Fsync.App.Bridge;
using Fsync.App.Notifications;
using Fsync.App.Services;
using Fsync.App.ViewModels;
using Fsync.App.Views;
using Fsync.Core;
using Microsoft.UI.Dispatching;

namespace Fsync.App;

/// <summary>
/// Parity features (phase 09): the trust-reset and account-recovery view models (shown by the window's
/// trust gate and, later, the pairing page) and toasts for incoming files: per-sender coalescing, no toast
/// while the Files page is in front, a click opens Files. Posting a toast never shows the window.
/// </summary>
public partial class App
{
    private IncomingFilesNoticeCoalescer? _notices;
    private DispatcherQueueTimer? _noticeTimer;

    /// <summary>Pending trust resets; null until the engine is ready.</summary>
    internal TrustResetPromptViewModel? TrustReset { get; private set; }
    /// <summary>"Lost your other devices?"; null until the engine is ready.</summary>
    internal AccountRecoveryViewModel? Recovery { get; private set; }

    partial void OnEngineReady()
    {
        TrustReset = new TrustResetPromptViewModel(Bridge);
        Recovery = new AccountRecoveryViewModel(Bridge);
        // The view model refreshes on the transition to Ready; the engine may be Ready already.
        if (Bridge.Phase == AccountPhase.Ready) _ = TrustReset.Refresh();

        _notices = new IncomingFilesNoticeCoalescer();
        _noticeTimer = _ui!.CreateTimer();
        _noticeTimer.IsRepeating = false;
        _noticeTimer.Tick += OnNoticeTimer;
        Transfers.OfferReceived += OnOfferReceived;
        ToastNotifier.Invoked += OnToastInvoked;
        ToastNotifier.Attach(_ui);

        _window?.ShowPhase(Bridge.Phase); // a window opened during startup gets its gate now
    }

    partial void OnQuitting()
    {
        ToastNotifier.Invoked -= OnToastInvoked;
        if (_noticeTimer is not null)
        {
            _noticeTimer.Stop();
            _noticeTimer.Tick -= OnNoticeTimer;
        }
        if (Transfers is not null) Transfers.OfferReceived -= OnOfferReceived;
        TrustReset?.Dispose();
    }

    /// <summary>The Files page in the active window shows the offer already (spec: suppression).</summary>
    private bool NoticesSuppressed => !IncomingFilesNotice.ShouldShow(IsWindowActive, VisiblePage == typeof(TransfersPage));

    private void OnOfferReceived(ClipItem offer, bool autoAccepted)
    {
        if (_quitting || !ToastNotifier.IsSupported || NoticesSuppressed) return;
        if (_notices!.Submit(IncomingFilesNotice.For(offer, autoAccepted)) is { } notice) ToastNotifier.Show(notice);
        ScheduleNoticeFlush();
    }

    private void OnNoticeTimer(DispatcherQueueTimer sender, object args)
    {
        if (_quitting || _notices is null) return;
        var due = _notices.Flush();
        if (!NoticesSuppressed)
            foreach (var notice in due) ToastNotifier.Show(notice);
        ScheduleNoticeFlush();
    }

    /// <summary>Wakes up when the next held notice is due (one-shot; re-armed after each flush).</summary>
    private void ScheduleNoticeFlush()
    {
        if (_notices?.NextDeadline is not { } deadline || _noticeTimer is null) return;
        // A little late on purpose: an early tick would find nothing due yet.
        var wait = deadline - DateTimeOffset.UtcNow + TimeSpan.FromMilliseconds(50);
        _noticeTimer.Stop();
        _noticeTimer.Interval = wait > TimeSpan.Zero ? wait : TimeSpan.FromMilliseconds(50);
        _noticeTimer.Start();
    }

    /// <summary>UI thread (marshalled by <see cref="ToastNotifier"/>).</summary>
    private void OnToastInvoked(ToastAction action)
    {
        if (_quitting) return;
        AppLog.Info($"toast: clicked {action}");
        if (action == ToastAction.OpenFiles) ShowWindow(MainWindow.FilesTag);
    }
}
