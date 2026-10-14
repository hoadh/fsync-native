using System.Diagnostics;

namespace Fsync.App.Services;

/// <summary>
/// Diagnostics log (data dir\app.log): <see cref="Trace"/> output with timestamps. Only names,
/// sizes, counts and states are logged, never clipboard content, file names or tokens.
/// The file restarts when it grows past 1 MiB.
/// </summary>
public static class AppLog
{
    private const long MaxBytes = 1024 * 1024;

    public static void Start(string path)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            if (File.Exists(path) && new System.IO.FileInfo(path).Length > MaxBytes) File.Delete(path);
            Trace.Listeners.Add(new TextWriterTraceListener(new StreamWriter(path, append: true) { AutoFlush = true })
            {
                TraceOutputOptions = TraceOptions.DateTime | TraceOptions.ThreadId,
            });
            Trace.AutoFlush = true;
            Info($"fsync started (pid {Environment.ProcessId})");
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException)
        {
            // No log is not a reason to stop the app.
        }
    }

    public static void Info(string message) => Trace.TraceInformation($"{DateTime.Now:HH:mm:ss.fff} {message}");
    public static void Warn(string message) => Trace.TraceWarning($"{DateTime.Now:HH:mm:ss.fff} {message}");
}
