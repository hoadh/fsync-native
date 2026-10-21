using Fsync.App.ViewModels;
using Microsoft.UI.Xaml.Controls;

namespace Fsync.App.Views;

public sealed partial class OnboardingPage : Page
{
    public OnboardingViewModel ViewModel { get; } = new(App.Current.Bridge);

    public OnboardingPage()
    {
        InitializeComponent();
        HeaderHost.Child = Ui.PageHeader("Sign in to fsync");
    }
}
