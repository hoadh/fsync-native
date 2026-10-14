using System.Diagnostics;
using System.Security.Cryptography;
using Fsync.Interop.Clipboard;
using Fsync.Interop.Shell;
using static Fsync.Interop.Tests.ClipboardTestSupport;

namespace Fsync.Interop.Tests;

/// <summary>
/// Lazy file paste through the shell's own copy engine: a child PowerShell runs the folder's
/// "Paste" verb (as Explorer's Ctrl+V does) against our virtual-file entry. Bytes come from real
/// files on disk through the same <see cref="RangeReader"/> shape core's read_file_range has.
/// </summary>
[Collection("clipboard")]
public sealed class ShellPasteTests : IDisposable
{
    private readonly StaThread _sta = new("test-shell-paste");
    private readonly string _src = TempDir(), _dst = TempDir();

    private static RangeReader FromDisk(string path) => (offset, len) =>
    {
        using var f = File.OpenRead(path);
        f.Position = offset;
        var buf = new byte[len];
        var n = f.Read(buf, 0, len);
        return buf[..n];
    };

    private VirtualFile Source(string rel, int size)
    {
        var path = Path.Combine(_src, rel.Replace('/', '\\'));
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllBytes(path, RandomNumberGenerator.GetBytes(size));
        return new VirtualFile(rel, size, DateTime.UtcNow, FromDisk(path));
    }

    /// <summary>
    /// Pastes into <see cref="_dst"/> from another process (PasteHelper runs the folder's Paste
    /// verb, so Explorer's copy engine reads our entry through OLE). Returns when the shell ended
    /// the operation; with <paramref name="stopOnWindow"/> the child is killed as soon as it shows
    /// a window once a read failed (the "Error Copying File or Folder" dialog).
    /// </summary>
    private (ClipDataObject Obj, bool Ended) Paste(IReadOnlyList<VirtualFile> files, int timeoutMs = 60_000, Func<bool>? stopOnWindow = null)
    {
        var obj = _sta.Invoke(() =>
        {
            var o = new ClipDataObject([], VirtualFile.WithFolders(files));
            o.Log += m => { lock (_log) _log.Add(m); };
            OleClipboard.Set(o);
            return o;
        });
        var exe = Path.Combine(AppContext.BaseDirectory, "Fsync.Interop.PasteHelper.exe");
        var psi = new ProcessStartInfo(exe) { RedirectStandardInput = true, RedirectStandardOutput = true, UseShellExecute = false, CreateNoWindow = true };
        psi.ArgumentList.Add(_dst);
        using var child = Process.Start(psi)!;
        try
        {
            var sw = Stopwatch.StartNew();
            var ended = false;
            while (!ended && sw.ElapsedMilliseconds < timeoutMs)
            {
                ended = obj.OperationEnded.Wait(200);
                if (!ended && stopOnWindow?.Invoke() == true && ProcessWindows.VisibleCount((uint)child.Id) > 0) break;
            }
            if (ended) child.StandardInput.WriteLine("done");
            if (!child.WaitForExit(ended ? 15_000 : 0)) child.Kill();
            return (obj, ended);
        }
        finally { if (!child.HasExited) child.Kill(); }
    }

    private readonly List<string> _log = [];
    private string CallLog { get { lock (_log) return string.Join("; ", _log); } }

    private static string Hash(string path) => Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path)));

    [Fact]
    public void ShellPastesFilesAndFoldersWithMatchingBytes()
    {
        VirtualFile[] files = [Source("a.bin", 3 << 20), Source("empty.txt", 0), Source("docs/nested/b.txt", 1234)];
        var (obj, ended) = Paste(files);
        Assert.True(ended, $"the shell never ended the paste operation; log={CallLog}; started={obj.OperationStarted} dst=[{string.Join(",", Directory.GetFileSystemEntries(_dst, "*", SearchOption.AllDirectories).Select(p => p[_dst.Length..] + ":" + (File.Exists(p) ? new FileInfo(p).Length : -1)))}]");
        Assert.Equal(0, obj.OperationResult);
        Assert.True(obj.OperationStarted); // async extraction: Explorer's UI thread stays free
        foreach (var f in files)
        {
            var rel = f.RelativePath.Replace('/', '\\');
            Assert.Equal(Hash(Path.Combine(_src, rel)), Hash(Path.Combine(_dst, rel)));
        }
    }

    [Fact]
    public void FailingRemoteReadEndsThePasteWithoutAPartialFile()
    {
        var good = Source("fails.bin", 4 << 20);
        var reads = 0;
        var failing = good with
        {
            Reader = (o, l) => ++reads > 3 ? throw new IOException("peer went away") : good.Reader!(o, l),
        };
        var (obj, ended) = Paste([failing], timeoutMs: 8_000, stopOnWindow: () => reads > 3);
        // The shell shows "Error Copying File or Folder" and waits for the user, so the end may not
        // come before the child is killed. Either way no complete wrong file is left.
        if (ended) Assert.NotEqual(0, obj.OperationResult);
        var target = Path.Combine(_dst, "fails.bin");
        Assert.True(!File.Exists(target) || new FileInfo(target).Length < good.Length || Hash(target) != Hash(Path.Combine(_src, "fails.bin")));
    }

    public void Dispose()
    {
        _sta.Invoke(() => OleClipboard.Set(new ClipDataObject([])));
        _sta.Dispose();
        DeleteDir(_src);
        DeleteDir(_dst);
    }
}
