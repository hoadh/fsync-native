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

    /// <summary>
    /// True once the pending trust resets were read after the latest transition to Ready (also when that
    /// read failed: a failure must not lock the user out). False in every other phase, so a window never
    /// shows the main UI between Ready and the first answer, whatever order the phase handlers run in.
    /// </summary>
    internal bool TrustChecked { get; private set; }
    /// <summary><see cref="TrustChecked"/> changed. UI thread.</summary>
    internal event Action? TrustCheckedChanged;
    private int _trustCheckSeq;

    partial void OnEngineReady()
    {
        TrustReset = new TrustResetPromptViewModel(Bridge);
        Recovery = new AccountRecoveryViewModel(Bridge);
        Bridge.PropertyChanged += OnBridgePhaseForTrust;
        // The view model refreshes on the transition to Ready; the engine may be Ready already.
        if (Bridge.Phase == AccountPhase.Ready) CheckTrust();

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
        if (TrustReset is not null) Bridge.PropertyChanged -= OnBridgePhaseForTrust;
        _trustCheckSeq++; // a check still in flight changes nothing
        TrustReset?.Dispose();
    }

    private void OnBridgePhaseForTrust(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(EngineBridge.Phase) || _quitting) return;
        if (Bridge.Phase == AccountPhase.Ready) CheckTrust();
        else
        {
            _trustCheckSeq++;
            SetTrustChecked(false);
        }
    }

    /// <summary>Reads the pending resets, then lets the window leave its checking state.</summary>
    private async void CheckTrust()
    {
        var seq = ++_trustCheckSeq;
        SetTrustChecked(false);
        try { await TrustReset!.Refresh(); } // logs and swallows engine failures itself
        catch (Exception e) { AppLog.Warn($"trust: first check failed {e.GetType().Name}"); }
        if (seq != _trustCheckSeq || _quitting) return;
        AppLog.Info($"trust: checked after Ready, pending={TrustReset!.HasPending}");
        SetTrustChecked(true);
    }

    private void SetTrustChecked(bool value)
    {
        if (TrustChecked == value) return;
        TrustChecked = value;
        TrustCheckedChanged?.Invoke();
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
