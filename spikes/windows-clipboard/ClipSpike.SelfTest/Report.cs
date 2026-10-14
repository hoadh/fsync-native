using System.Runtime.InteropServices;

namespace ClipSpike.SelfTest;

/// <summary>PASS/FAIL/INFO lines to the console and to results/selftest-*.txt.</summary>
internal sealed class Report : IDisposable
{
    private readonly StreamWriter _file;
    public int Failures { get; private set; }

    public Report(string dir)
    {
        Directory.CreateDirectory(dir);
        var path = Path.Combine(dir, $"selftest-{DateTime.Now:yyyyMMdd-HHmmss}-{RuntimeInformation.ProcessArchitecture}.txt".ToLowerInvariant());
        _file = new StreamWriter(path) { AutoFlush = true };
        Info($"os {Environment.OSVersion.Version} ({RuntimeInformation.OSDescription}), process {RuntimeInformation.ProcessArchitecture}, .NET {Environment.Version}");
        Info($"results: {path}");
    }

    public void Check(string name, bool ok, string detail)
    {
        if (!ok) Failures++;
        Write($"{(ok ? "PASS" : "FAIL")} {name}: {detail}");
    }

    public void Info(string text) => Write($"INFO {text}");

    private void Write(string line)
    {
        Console.WriteLine(line);
        _file.WriteLine(line);
    }

    public void Dispose() => _file.Dispose();
}
