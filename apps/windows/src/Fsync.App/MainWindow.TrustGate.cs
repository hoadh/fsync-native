using System.ComponentModel;
using Fsync.App.Bridge;
using Fsync.App.ViewModels;
using Fsync.App.Views;
using Microsoft.UI.Xaml;

namespace Fsync.App;

/// <summary>
/// Trust gate (phase 09): while the account is Ready the window shows only <see cref="TrustResetPage"/>
/// (no nav pane) when another device's trust reset waits for an answer, and also
/// <list type="bullet">
/// <item>until the first check after Ready finished (the page shows a progress ring; before the view
/// model exists the starting overlay covers the window instead);</item>
/// <item>while an answer is in flight, and until its outcome or message is dismissed.</item>
/// </list>
/// Every window, also one recreated from the tray, checks the gate in ShowPhase. A page asked for while
/// gated (tray, toast click) opens when the gate closes.
/// </summary>
public sealed partial class MainWindow
{
    private TrustResetPromptViewModel? _trust;
    private bool _gated;
    /// <summary>Page requested via <see cref="App.ShowWindow"/> and not opened yet because of the gate.</summary>
    private string? _deferredTag;

    partial void OnWindowCreatedExtra()
    {
        App.Current.TrustCheckedChanged += OnTrustCheckedChanged;
        SubscribeTrust();
    }

    partial void OnWindowClosedExtra()
    {
        App.Current.TrustCheckedChanged -= OnTrustCheckedChanged;
        if (_trust is not null) _trust.PropertyChanged -= OnTrustChanged;
        _trust = null;
        if (ContentFrame.Content is TrustResetPage page) page.Detach(); // Unloaded is not reliable on close
    }

    /// <summary>Remembers <paramref name="page"/> in case the gate holds it back; null keeps an earlier request.</summary>
    internal void RequestPage(string? page)
    {
        if (page is not null) _deferredTag = page;
    }

    partial void OnShowPhaseGate(AccountPhase phase, ref bool gated)
    {
        SubscribeTrust(); // the view model may have been created after this window (startup)
        _gated = gated = IsGated(phase);
        if (!gated)
        {
            // Routing runs next: a page held back by the gate opens now (an explicit page still wins).
            if (_deferredTag is { } tag && phase is AccountPhase.Ready or AccountPhase.Offline) _pendingTag = tag;
            _deferredTag = null;
            return;
        }

        Nav.IsPaneVisible = false;
        if (_pendingTag is { } pending) // a Settings shortcut replayed in Nav.Loaded must not leave the gate
        {
            _deferredTag ??= pending;
            _pendingTag = null;
        }
        if (_trust is null)
        {
            // Ready, but the view model is not created yet: keep the starting overlay over everything.
            Overlay.Visibility = Visibility.Visible;
            Spinner.IsActive = true;
            OverlayText.Text = "Checking the device list...";
            ResetHistoryButton.Visibility = Visibility.Collapsed;
            return;
        }
        Navigate(typeof(TrustResetPage)); // no rebuild when it is shown already
        if (ContentFrame.Content is TrustResetPage page) page.SetChecking(!App.Current.TrustChecked);
    }

    private bool IsGated(AccountPhase phase)
    {
        if (phase != AccountPhase.Ready) return false;
        if (_trust is not { } t || !App.Current.TrustChecked) return true; // first check still running
        return t.HasPending
            || t.Outcome is TrustResetOutcome.Trusted or TrustResetOutcome.Declined
            // HasPending drops before Outcome is set while an answer runs; an error message stays until dismissed.
            || _gated && (t.IsBusy || !string.IsNullOrEmpty(t.Message));
    }

    private void SubscribeTrust()
    {
        if (_trust is not null || App.Current.TrustReset is not { } vm) return;
        _trust = vm;
        _trust.PropertyChanged += OnTrustChanged;
    }

    /// <summary>Re-routes only when the gate opens or closes (HasPending is raised on every refresh).</summary>
    private void OnTrustChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is not (nameof(TrustResetPromptViewModel.HasPending) or nameof(TrustResetPromptViewModel.Outcome)
            or nameof(TrustResetPromptViewModel.IsBusy) or nameof(TrustResetPromptViewModel.Message))) return;
        var phase = App.Current.Bridge.Phase;
        if (IsGated(phase) != _gated) ShowPhase(phase);
    }

    /// <summary>The first check finished or restarted: leave the checking state (or the gate).</summary>
    private void OnTrustCheckedChanged()
    {
        var phase = App.Current.Bridge.Phase;
        if (_gated || IsGated(phase)) ShowPhase(phase);
    }
}
