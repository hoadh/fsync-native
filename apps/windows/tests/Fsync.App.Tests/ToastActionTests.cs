using Fsync.App.ViewModels;

namespace Fsync.App.Tests;

public sealed class ToastActionTests
{
    [Fact]
    public void OnlyTheExactConstantParses()
    {
        Assert.True(ToastActions.TryParse("action=open-files", out var action));
        Assert.Equal(ToastAction.OpenFiles, action);
        Assert.Equal("action=open-files", ToastActions.ToArgs(ToastAction.OpenFiles));
    }

    [Fact]
    public void BuilderKeyAndValueJoinToTheParsedConstant()
    {
        // The toast is built from ArgumentKey/OpenFilesValue; a click must parse back to OpenFiles.
        Assert.Equal("action", ToastActions.ArgumentKey);
        Assert.Equal("open-files", ToastActions.OpenFilesValue);
        Assert.True(ToastActions.TryParse($"{ToastActions.ArgumentKey}={ToastActions.OpenFilesValue}", out var action));
        Assert.Equal(ToastAction.OpenFiles, action);
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("action=Open-Files")]
    [InlineData(" action=open-files")]
    [InlineData("action=open-files ")]
    [InlineData("action=open-files&path=C:\\Windows")]
    [InlineData("action=open-file")]
    [InlineData("open-files")]
    [InlineData("action=delete")]
    public void AnythingElseIsIgnored(string? args)
    {
        Assert.False(ToastActions.TryParse(args, out var action));
        Assert.Equal(default, action);
    }
}
