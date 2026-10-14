using ClipSpike.SelfTest;

// Phase-05 self-test. Without arguments runs all checks; the other modes are child processes.
//   ClipSpike.SelfTest [--quick]        all checks (--quick skips the 200 MiB paste)
//   --read | --shell-paste <dir> | --own-and-exit none|flush|clear
switch (args.FirstOrDefault())
{
    case "--read":
        return Children.Read();
    case "--shell-paste":
        return Children.ShellPaste(args[1]);
    case "--own-and-exit":
        return Children.OwnAndExit(args[1]);
}

// results/ next to the projects (spikes/windows-clipboard/results), whatever the bin layout.
var spikeRoot = new DirectoryInfo(AppContext.BaseDirectory);
while (spikeRoot.Parent is not null && !Directory.Exists(Path.Combine(spikeRoot.FullName, "ClipSpike.Core"))) spikeRoot = spikeRoot.Parent;
using var report = new Report(Path.Combine(spikeRoot.FullName, "results"));
using (var listener = new ListenerChecks(report)) listener.Run();
using (var paste = new PasteChecks(report, large: !args.Contains("--quick"))) paste.Run();
report.Info($"{report.Failures} failure(s)");
return report.Failures == 0 ? 0 : 1;
