using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.ViewModels;

public enum OnboardingStep { Email, Code }

/// <summary>Email OTP sign-in. Success leaves the bridge in Ready or NeedsPairing.</summary>
public sealed partial class OnboardingViewModel(EngineBridge bridge) : ObservableObject
{
    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(SendCodeCommand))]
    private string _email = bridge.Email ?? "";

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(VerifyCommand))]
    private string _code = "";

    [ObservableProperty] private OnboardingStep _step = OnboardingStep.Email;
    [ObservableProperty] private string? _error;

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(SendCodeCommand), nameof(VerifyCommand))]
    private bool _isBusy;

    public bool CanSendCode => !IsBusy && IsPlausibleEmail(Email);
    public bool CanVerify => !IsBusy && Code.Trim().Length >= 4;

    [RelayCommand(CanExecute = nameof(CanSendCode))]
    private async Task SendCodeAsync()
    {
        await Busy(async () =>
        {
            await bridge.LoginStartAsync(Email.Trim());
            Code = "";
            Step = OnboardingStep.Code;
        });
    }

    [RelayCommand(CanExecute = nameof(CanVerify))]
    private async Task VerifyAsync()
    {
        await Busy(() => bridge.LoginVerifyAsync(Email.Trim(), Code.Trim()));
    }

    [RelayCommand]
    private void Back()
    {
        Step = OnboardingStep.Email;
        Error = null;
    }

    private async Task Busy(Func<Task> work)
    {
        IsBusy = true;
        Error = null;
        try { await work(); }
        catch (FsyncException.Auth) { Error = "That code is wrong or has expired."; }
        catch (FsyncException e) { Error = UserMessages.For(e); }
        finally { IsBusy = false; }
    }

    internal static bool IsPlausibleEmail(string s)
    {
        s = s.Trim();
        var at = s.IndexOf('@');
        return at > 0 && at < s.Length - 3 && s.IndexOf('.', at) > at + 1 && !s.Contains(' ');
    }
}
