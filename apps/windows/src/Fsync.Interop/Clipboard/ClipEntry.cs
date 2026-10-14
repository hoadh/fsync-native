namespace Fsync.Interop.Clipboard;

/// <summary>
/// One HGLOBAL format of a clipboard entry we own. Eager entries carry their bytes; lazy ones
/// render on the first request (delayed rendering: a target pastes, or the entry is flushed).
/// Render is called on the owning STA thread and may block (network fetch).
/// </summary>
public sealed class ClipEntry
{
    private readonly Func<byte[]> _render;
    private readonly object _lock = new();
    private byte[]? _bytes;

    private ClipEntry(uint format, long size, byte[]? bytes, Func<byte[]>? render)
    {
        Format = format;
        Size = size;
        _bytes = bytes;
        _render = render ?? (() => bytes!);
    }

    public uint Format { get; }
    /// <summary>Expected size in bytes (for the exit policy).</summary>
    public long Size { get; }
    public bool IsRendered { get { lock (_lock) return _bytes is not null; } }

    public static ClipEntry Eager(uint format, byte[] bytes) => new(format, bytes.Length, bytes, null);
    public static ClipEntry Eager(string format, byte[] bytes) => Eager(ClipboardFormats.Register(format), bytes);
    public static ClipEntry Lazy(uint format, long size, Func<byte[]> render) => new(format, size, null, render);
    public static ClipEntry Lazy(string format, long size, Func<byte[]> render) => Lazy(ClipboardFormats.Register(format), size, render);

    /// <summary>
    /// Flags for content from another device: not for clipboard monitors, Win+V history or the
    /// cloud clipboard. Our own listener still sees FsyncOrigin first (OwnEcho).
    /// </summary>
    public static ClipEntry[] PrivacyFlags() =>
    [
        Eager(ClipboardFormats.ExcludeFromMonitor, BitConverter.GetBytes(1u)),
        Eager(ClipboardFormats.CanIncludeInHistory, BitConverter.GetBytes(0u)),
        Eager(ClipboardFormats.CanUploadToCloud, BitConverter.GetBytes(0u)),
    ];

    /// <summary>The bytes, rendering once; a failed render is retried on the next request.</summary>
    public byte[] Render()
    {
        lock (_lock) return _bytes ??= _render();
    }
}
