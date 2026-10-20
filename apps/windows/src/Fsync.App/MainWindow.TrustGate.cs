using System.ComponentModel;
using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Fsync.App.Views;

namespace Fsync.App;

/// <summary>
/// Trust gate (phase 09): while the account is Ready and another device's trust reset waits for an
/// answer, the window shows only <see cref="TrustResetPage"/> (no nav pane). The page stays until its
/// outcome is dismissed. Every window, also one recreated from the tray, checks the gate in ShowPhase.
/// </summary>
public sealed partial class MainWindow
{
    private TrustResetPromptViewModel? _trust;
    private bool _gated;

    partial void OnWindowCreatedExtra() => SubscribeTrust();

    partial void OnWindowClosedExtra()
    {
        if (_trust is not null) _trust.PropertyChanged -= OnTrustChanged;
        _trust = null;
    }

    partial void OnShowPhaseGate(AccountPhase phase, ref bool gated)
    {
        SubscribeTrust(); // the view model may have been created after this window (startup)
        _gated = gated = IsGated(phase);
        if (!gated) return;
        Nav.IsPaneVisible = false;
        _pendingTag = null; // a Settings shortcut replayed in Nav.Loaded must not leave the gate
        Navigate(typeof(TrustResetPage));
    }

    private bool IsGated(AccountPhase phase) =>
        phase == AccountPhase.Ready && _trust is { } t
        && (t.HasPending || t.Outcome is TrustResetOutcome.Trusted or TrustResetOutcome.Declined);

    private void SubscribeTrust()
    {
        if (_trust is not null || App.Current.TrustReset is not { } vm) return;
        _trust = vm;
        _trust.PropertyChanged += OnTrustChanged;
    }

    /// <summary>Re-routes only when the gate opens or closes (HasPending is raised on every refresh).</summary>
    private void OnTrustChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is not (nameof(TrustResetPromptViewModel.HasPending) or nameof(TrustResetPromptViewModel.Outcome))) return;
        var phase = App.Current.Bridge.Phase;
        if (IsGated(phase) != _gated) ShowPhase(phase);
    }
}
