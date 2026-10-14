using Fsync.Interop.Clipboard;

// Usage: Fsync.Interop.PasteHelper <folder>
// Pastes the clipboard into <folder> with the shell's Paste verb, keeps pumping (the copy engine
// runs on this process's threads) and exits when stdin gets a line. Prints "PASTING" first.
if (args.Length != 1 || !Directory.Exists(args[0]))
{
    Console.Error.WriteLine("usage: Fsync.Interop.PasteHelper <existing folder>");
    return 2;
}
using var sta = new StaThread("paste");
try
{
    sta.Invoke(() =>
    {
        var type = Type.GetTypeFromProgID("Shell.Application") ?? throw new InvalidOperationException("no Shell.Application");
        dynamic shell = Activator.CreateInstance(type)!;
        shell.NameSpace(args[0]).Self.InvokeVerb("Paste");
    });
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
