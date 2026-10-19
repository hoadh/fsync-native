using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.App.Services;
using Fsync.Core;

namespace Fsync.App.ViewModels;

/// <summary>
/// State and actions behind the trust-reset prompt (ADR 012, macOS TrustResetPromptModel). Holds the
/// bridge, never an engine: bridge events survive the ResetHistoryAsync engine recreate, and the Ready
/// transition refreshes. Answers use the reset id the user saw. UI thread only.
/// </summary>
public sealed partial class TrustResetPromptViewModel : ObservableObject, IDisposable
{
    private readonly EngineBridge _bridge;
    /// <summary>Every refresh takes a number; only the newest one may apply its result.</summary>
    private int _seq;
    private IReadOnlyList<TrustResetInfo> _pending = [];
    private string? _confirmingResetId;

    [ObservableProperty] private bool _isBusy;
    [ObservableProperty] private TrustResetOutcome _outcome;
    /// <summary>Short text for the last failure or notice, or null.</summary>
    [ObservableProperty] private string? _message;
    /// <summary>True when <see cref="Message"/> is informational (the request changed), not a failure.</summary>
    [ObservableProperty] private bool _messageIsNotice;

    /// <summary>The confirmation dialog closes: the displayed reset changed while it was open.</summary>
    public event Action? ConfirmAborted;

    public TrustResetPromptViewModel(EngineBridge bridge)
    {
        _bridge = bridge;
        _bridge.AccountEventReceived += OnAccountEvent;
        _bridge.PropertyChanged += OnBridgeChanged;
    }

    /// <summary>Resets waiting for an answer; the prompt shows the first one.</summary>
    public IReadOnlyList<TrustResetInfo> Pending => _pending;
    public TrustResetInfo? Current => _pending.Count > 0 ? _pending[0] : null;
    public bool HasPending => Current is not null;
    /// <summary>Peer-supplied names of the shown reset, sanitized.</summary>
    public IReadOnlyList<string> DisplayNames => Current?.deviceNames.Select(n => PeerText.DisplayName(n)).ToArray() ?? [];
    /// <summary>The reset the confirmation dialog was opened for, or null when it is closed.</summary>
    public string? ConfirmingResetId => _confirmingResetId;

    /// <summary>Re-reads the pending resets. Overlapping calls: the older result is dropped.</summary>
    public async Task Refresh()
    {
        var seq = ++_seq;
        try
        {
            var list = await _bridge.RunAsync(e => e.PendingTrustResets());
            if (seq == _seq) Apply(list);
        }
        catch (FsyncException e) { AppLog.Warn($"trust: pending resets failed: {e.GetType().Name}"); }
        catch (InvalidOperationException) { AppLog.Info("trust: refresh skipped, no engine"); }
    }

    /// <summary>Opens the confirmation for the shown reset. False (nothing opens) for any other id or while busy.</summary>
    public bool BeginConfirm(string resetId)
    {
        if (IsBusy || Current?.resetId != resetId) return false;
        SetConfirming(resetId);
        return true;
    }

    public void CancelConfirm() => SetConfirming(null);

    /// <summary>Trusts exactly <paramref name="resetId"/>; a no-op unless the dialog was opened for it.</summary>
    [RelayCommand]
    public Task ConfirmAsync(string resetId)
    {
        if (resetId is null || resetId != _confirmingResetId) return Task.CompletedTask;
        SetConfirming(null);
        return AnswerAsync(resetId, e => e.ConfirmTrustReset(resetId));
    }

    /// <summary>Keeps this PC's current devices; <paramref name="resetId"/> is the reset the user saw.</summary>
    [RelayCommand]
    public Task DeclineAsync(string resetId)
    {
        if (resetId is null) return Task.CompletedTask;
        return AnswerAsync(resetId, e => { e.DeclineTrustReset(resetId); return null; });
    }

    /// <summary>Hides the outcome so the next pending reset (if any) shows.</summary>
    public void DismissOutcome()
    {
        Outcome = TrustResetOutcome.Unanswered;
        ClearMessage();
    }

    /// <summary>Stores a new list. When the shown reset changes the old message no longer applies; a replaced reset gets the notice.</summary>
    internal void Apply(IReadOnlyList<TrustResetInfo> list)
    {
        var oldId = Current?.resetId;
        _pending = list;
        OnPropertyChanged(nameof(Pending));
        OnPropertyChanged(nameof(Current));
        OnPropertyChanged(nameof(HasPending));
        OnPropertyChanged(nameof(DisplayNames));
        var newId = Current?.resetId;
        if (newId == oldId) return;
        ClearMessage();
        if (_confirmingResetId is not null && _confirmingResetId != newId)
        {
            SetConfirming(null);
            ConfirmAborted?.Invoke();
        }
        if (oldId is not null && newId is not null) Show(TrustResetMessages.ListChanged);
    }

    /// <param name="call">Returns the account state (confirm) or null (decline).</param>
    private async Task AnswerAsync(string id, Func<IFsyncEngine, AccountState?> call)
    {
        if (IsBusy) return;
        IsBusy = true;
        ClearMessage();
        try
        {
            var state = await _bridge.RunAsync(call);
            await Refresh();
            Outcome = state is { } s ? TrustResetMessages.OutcomeFor(s) : TrustResetOutcome.Declined;
            if (state is { } applied) _bridge.ApplyAccountState(applied);
        }
        catch (FsyncException.NotFound e)
        {
            await Refresh(); // first: it may clear a message when the shown reset changes
            Show(TrustResetMessages.For(e));
        }
        catch (Exception e) when (e is FsyncException or InvalidOperationException)
        {
            // Otherwise the shown reset changed during the call; its "list changed" notice stays.
            if (Current?.resetId == id) Show(TrustResetMessages.For(e));
        }
        finally { IsBusy = false; }
    }

    private void OnAccountEvent(AccountEvent ev)
    {
        if (ev is AccountEvent.TrustReset) _ = Refresh();
    }

    private void OnBridgeChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(EngineBridge.Phase) && _bridge.Phase == AccountPhase.Ready) _ = Refresh();
    }

    private void SetConfirming(string? id)
    {
        if (_confirmingResetId == id) return;
        _confirmingResetId = id;
        OnPropertyChanged(nameof(ConfirmingResetId));
    }

    private void Show(TrustResetMessage m)
    {
        Message = m.Text;
        MessageIsNotice = m.IsNotice;
    }

    private void ClearMessage()
    {
        Message = null;
        MessageIsNotice = false;
    }

    public void Dispose()
    {
        _bridge.AccountEventReceived -= OnAccountEvent;
        _bridge.PropertyChanged -= OnBridgeChanged;
    }
}
