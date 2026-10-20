namespace Fsync.App.ViewModels;

/// <summary>What a toast click may ask for. A fixed set: toast arguments are never interpreted beyond it.</summary>
public enum ToastAction
{
    /// <summary>Open the Files page.</summary>
    OpenFiles,
}

/// <summary>Builds and parses toast arguments (spec section 4: fixed constant set).</summary>
public static class ToastActions
{
    /// <summary>Key and value of the one toast argument (AppNotificationBuilder.AddArgument takes them apart).</summary>
    public const string ArgumentKey = "action", OpenFilesValue = "open-files";

    /// <summary>The only argument string a toast carries.</summary>
    public const string OpenFilesArgs = ArgumentKey + "=" + OpenFilesValue;

    public static string ToArgs(ToastAction action) => action switch
    {
        ToastAction.OpenFiles => OpenFilesArgs,
        _ => throw new ArgumentOutOfRangeException(nameof(action)),
    };

    /// <summary>True only for the exact constant; anything else (null, extra keys, case, spaces) is ignored.</summary>
    public static bool TryParse(string? args, out ToastAction action)
    {
        action = default;
        if (!string.Equals(args, OpenFilesArgs, StringComparison.Ordinal)) return false;
        action = ToastAction.OpenFiles;
        return true;
    }
}
