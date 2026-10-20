using CommunityToolkit.Mvvm.ComponentModel;
using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.ViewModels;

public enum RecoveryStep
{
    Idle,
    /// <summary>Explains what will happen.</summary>
    Explain,
    /// <summary>Last chance; only here does <see cref="AccountRecoveryViewModel.ResetAsync"/> call the engine.</summary>
    FinalConfirm,
}

/// <summary>
/// "Lost your other devices?" for a PC that needs pairing with no device to pair with (macOS
/// AccountRecoveryModel). <c>ResetAccountTrust</c> makes this PC the only trusted device, so it takes
/// two explicit steps. The result goes through the bridge's account-state mapping. UI thread only.
/// </summary>
public sealed class AccountRecoveryViewModel(EngineBridge bridge) : ObservableObject
{
    private RecoveryStep _step;
    private bool _isBusy;
    private string? _error;

    public RecoveryStep Step
    {
        get => _step;
        private set { if (SetProperty(ref _step, value)) OnPropertyChanged(nameof(IsPresented)); }
    }

    public bool IsBusy { get => _isBusy; private set => SetProperty(ref _isBusy, value); }
    public string? Error { get => _error; private set => SetProperty(ref _error, value); }
    public bool IsPresented => Step != RecoveryStep.Idle;

    public void Begin()
    {
        if (IsBusy) return;
        Error = null;
        Step = RecoveryStep.Explain;
    }

    /// <summary>Moves from the explanation to the final confirmation; ignored from any other step.</summary>
    public void Proceed()
    {
        if (Step != RecoveryStep.Explain || IsBusy) return;
        Step = RecoveryStep.FinalConfirm;
    }

    public void Cancel()
    {
        if (IsBusy) return;
        Step = RecoveryStep.Idle;
        Error = null;
    }

    /// <summary>Calls <c>ResetAccountTrust</c>, only from the final confirmation.</summary>
    public async Task ResetAsync()
    {
        if (Step != RecoveryStep.FinalConfirm || IsBusy) return;
        IsBusy = true;
        Error = null;
        try
        {
            var state = await bridge.RunAsync(e => e.ResetAccountTrust());
            Step = RecoveryStep.Idle;
            bridge.ApplyAccountState(state);
        }
        catch (FsyncException e) when (e is FsyncException.Auth or FsyncException.ReauthRequired)
        {
            Step = RecoveryStep.Idle;
            bridge.RequireSignIn();
        }
        catch (FsyncException e) { Error = UserMessages.For(e); }
        catch (InvalidOperationException) { Error = TrustResetMessages.Other.Text; } // no engine (quit, history reset)
        finally { IsBusy = false; }
    }
}
