using CommunityToolkit.Mvvm.ComponentModel;
using Fsync.Core;

namespace Fsync.App.Files;

/// <summary>One row of the transfer list (a pull or a push), updated from TransferEvents.</summary>
public sealed partial class TransferItem : ObservableObject
{
    public TransferItem(ulong id, string title, TransferDirection direction)
    {
        Id = id;
        _title = title;
        Direction = direction;
    }

    public ulong Id { get; }
    /// <summary>What and where ("report.pdf to Mac mini").</summary>
    [ObservableProperty] private string _title;
    public TransferDirection Direction { get; }

    [ObservableProperty] private TransferState _state = TransferState.Started;
    [ObservableProperty] private double _percent;
    [ObservableProperty] private string _detail = "Starting...";
    /// <summary>Final path of a received file (Open / Show in folder).</summary>
    [ObservableProperty] private string? _path;
    /// <summary>A failed pull that can be started again (Retry).</summary>
    [ObservableProperty] private bool _canRetry;
    /// <summary>Core's message of the last failure (diagnostics, retry decisions).</summary>
    public string? FailureMessage { get; private set; }

    public bool IsActive => State is TransferState.Started or TransferState.Progress;

    public void Apply(TransferEvent ev)
    {
        State = ev.state;
        if (ev.bytesTotal > 0) Percent = Math.Min(100, 100.0 * ev.bytesDone / ev.bytesTotal);
        if (ev.path is { } p) Path = p;
        if (ev.state == TransferState.Failed) FailureMessage = ev.message;
        Detail = ev.state switch
        {
            TransferState.Done => Direction == TransferDirection.Send ? "Sent" : "Received",
            TransferState.Failed => $"Failed{(ev.message is { Length: > 0 } m ? $": {m}" : "")}",
            TransferState.Cancelled => "Cancelled",
            _ => $"{Size(ev.bytesDone)} of {Size(ev.bytesTotal)}",
        };
        OnPropertyChanged(nameof(IsActive));
    }

    public static string Size(ulong bytes) => bytes switch
    {
        < 1024 => $"{bytes} B",
        < 1024 * 1024 => $"{bytes / 1024.0:0.#} KB",
        < 1024UL * 1024 * 1024 => $"{bytes / (1024.0 * 1024):0.#} MB",
        _ => $"{bytes / (1024.0 * 1024 * 1024):0.##} GB",
    };

    /// <summary>Accessible name for list rows.</summary>
    public override string ToString() => $"{Title}, {Detail}";
}

/// <summary>Files a peer pushed to this PC, waiting for Accept / Decline.</summary>
public sealed record IncomingOffer(ClipItem Item)
{
    public string Title => Item.files.Length == 1
        ? $"{Item.fromName} sent {Item.files[0].name} ({TransferItem.Size(Item.totalSize)})"
        : $"{Item.fromName} sent {Item.files.Length} files ({TransferItem.Size(Item.totalSize)})";

    public override string ToString() => Title;
}
