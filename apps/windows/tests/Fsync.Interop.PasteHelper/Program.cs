using Fsync.Interop.Clipboard;

// Usage: Fsync.Interop.PasteHelper <folder> [--warm]
// Pastes the clipboard into <folder> with the shell's Paste verb, keeps pumping (the copy engine
// runs on this process's threads) and exits when stdin gets a line. Prints "PASTING" first.
// --warm: sets up the shell objects first, prints "READY" and waits for a stdin line ("paste")
// before pasting, so a caller can pay the process and COM start-up before the clipboard is ready.
var warm = args.Length == 2 && args[1] == "--warm";
if (!(args.Length == 1 || warm) || !Directory.Exists(args[0]))
{
    Console.Error.WriteLine("usage: Fsync.Interop.PasteHelper <existing folder> [--warm]");
    return 2;
}
using var sta = new StaThread("paste");
try
{
    dynamic? folder = null;
    sta.Invoke(() =>
    {
        var type = Type.GetTypeFromProgID("Shell.Application") ?? throw new InvalidOperationException("no Shell.Application");
        dynamic shell = Activator.CreateInstance(type)!;
        folder = shell.NameSpace(args[0]).Self;
    });
    if (warm)
    {
        Console.WriteLine("READY");
        Console.Out.Flush();
        if (Console.ReadLine() is null) return 0; // the caller went away without pasting
    }
    sta.Invoke(() => folder!.InvokeVerb("Paste"));
}
catch (Exception e)
{
    Console.Error.WriteLine($"paste failed: {e.Message}");
    return 1;
}
Console.WriteLine("PASTING");
Console.Out.Flush();
Console.ReadLine();
return 0;
