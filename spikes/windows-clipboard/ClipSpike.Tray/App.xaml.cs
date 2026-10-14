using H.NotifyIcon;
using H.NotifyIcon.Core;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace ClipSpike.Tray;

/// <summary>
/// Tray-only WinUI 3 app (no window): H.NotifyIcon icon + menu, clipboard listener on the UI
/// thread. <c>--smoke [log]</c> runs a self-check and exits (0 = pass), log under %TEMP%.
/// </summary>
public partial class App : Application
{
    private TaskbarIcon? _icon;
    private ClipboardListener? _listener;
    private readonly List<(uint Seq, SkipReason Reason, bool OnUiThread)> _events = [];
    private int _uiThread;
    private MenuFlyoutItem? _status;

    public App() => InitializeComponent();

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _uiThread = Environment.CurrentManagedThreadId;
        OleClipboard.InitializeSta(); // UI thread is STA; OleSetClipboard also needs OLE initialized
        _icon = CreateIcon();
        _listener = new ClipboardListener();
        _listener.Changed += seq =>
        {
            var (_, reason) = SensitiveFormatFilter.Inspect();
            _events.Add((seq, reason, Environment.CurrentManagedThreadId == _uiThread));
            if (_status is not null) _status.Text = $"Clipboard updates: {_events.Count} (last: {reason})";
        };

        var cli = Environment.GetCommandLineArgs();
        var smoke = Array.IndexOf(cli, "--smoke");
        if (smoke >= 0)
        {
            var log = cli.ElementAtOrDefault(smoke + 1) ?? Path.Combine(Path.GetTempPath(), "clipspike-tray-smoke.log");
            _ = SmokeTest.RunAsync(this, DispatcherQueue.GetForCurrentThread(), log);
        }
    }

    private TaskbarIcon CreateIcon()
    {
        _status = new MenuFlyoutItem { Text = "Clipboard updates: 0", IsEnabled = false };
        var exit = new MenuFlyoutItem { Text = "Exit" };
        exit.Click += (_, _) => Shutdown();
        var menu = new MenuFlyout();
        menu.Items.Add(_status);
        menu.Items.Add(new MenuFlyoutSeparator());
        // Manual app matrix (phase 05): put a lazy entry on the clipboard, then paste elsewhere.
        menu.Items.Add(Offer("Offer 3 test files (lazy)", [("notes.txt", 1_234), ("photo.jpg", 5L << 20), ("report.pdf", 300_000)]));
        menu.Items.Add(Offer("Offer 200 MiB test file (lazy, 2 ms/read)", [("big.bin", 200L << 20)]));
        menu.Items.Add(new MenuFlyoutSeparator());
        menu.Items.Add(exit);

        var icon = new TaskbarIcon
        {
            ToolTipText = "fsync clipboard spike",
            ContextFlyout = menu,
            ContextMenuMode = ContextMenuMode.SecondWindow,
            IconSource = new GeneratedIconSource
            {
                Text = "F",
                Foreground = new SolidColorBrush(Color.FromArgb(255, 255, 255, 255)),
                Background = new SolidColorBrush(Color.FromArgb(255, 0, 99, 177)),
            },
        };
        icon.ForceCreate(enablesEfficiencyMode: false);
        return icon;
    }

    private VirtualFileDataObject? _offered;

    private MenuFlyoutItem Offer(string label, (string Name, long Size)[] files)
    {
        var item = new MenuFlyoutItem { Text = label };
        item.Click += (_, _) =>
        {
            // Synthetic content: byte i = i * 31 mod 256, with a small per-read delay.
            var vfs = files.Select(f => new VirtualFile(f.Name, f.Size, DateTime.UtcNow, (off, n) =>
            {
                Thread.Sleep(2);
                var b = new byte[n];
                for (var i = 0; i < n; i++) b[i] = unchecked((byte)((off + i) * 31));
                return b;
            })).ToList();
            _offered = new VirtualFileDataObject(vfs);
            OleClipboard.Set(_offered);
        };
        return item;
    }

    internal IReadOnlyList<(uint Seq, SkipReason Reason, bool OnUiThread)> Events => _events;
    internal bool IconCreated => _icon?.IsCreated == true;

    internal void Shutdown()
    {
        // Exit policy (ADR 010): a lazy entry cannot outlive us, so remove it instead of flushing.
        if (_offered is not null) OleClipboard.ClearIfCurrent(_offered);
        _listener?.Dispose();
        _icon?.Dispose();
        Exit();
    }
}
