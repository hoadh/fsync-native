using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Fsync.App.Bridge;
using Fsync.Core;

namespace Fsync.App.ViewModels;

public enum PairingStage { Idle, Offering, Joining, ConfirmSas, WaitingForOther, Completed, Failed }

/// <summary>
/// Pairing on both sides. A new device (NeedsPairing) offers a code or QR and gets the SAS via
/// ConfirmRequest; an existing device joins with that code and gets the SAS from pairing_join.
/// Both must compare the words and accept (ffi-contract: pairing).
/// </summary>
public sealed partial class PairingViewModel : ObservableObject, IDisposable
{
    private readonly EngineBridge _bridge;
    private readonly Func<DateTimeOffset> _now;
    private ulong? _requestId;

    [ObservableProperty] private PairingStage _stage = PairingStage.Idle;
    [ObservableProperty] private string? _offerCode;
    [ObservableProperty] private string? _qrPayload;
    [ObservableProperty] private byte[]? _qrPng;
    [ObservableProperty] private DateTimeOffset? _expiresAt;
    [ObservableProperty] private string _joinInput = "";
    [ObservableProperty] private string? _sasWords;
    [ObservableProperty] private string? _peerName;
    [ObservableProperty] private string? _peerPlatform;
    [ObservableProperty] private string? _error;

    public PairingViewModel(EngineBridge bridge, Func<DateTimeOffset>? now = null)
    {
        _bridge = bridge;
        _now = now ?? (() => DateTimeOffset.Now);
        _bridge.PairingEventReceived += OnPairingEvent;
    }

    /// <summary>This device is the new one: show a code (or QR) for an existing device to enter.</summary>
    [RelayCommand]
    public async Task OfferAsync(PairingMode mode)
    {
        await CancelQuietly();
        Reset();
        try
        {
            var o = await _bridge.RunAsync(e => e.PairingOffer(mode));
            (OfferCode, QrPayload) = (o.code, o.qrPayload);
            QrPng = o.qrPayload is { } p ? QrImage.Png(p) : null;
            ExpiresAt = DateTimeOffset.FromUnixTimeMilliseconds((long)o.expiresMs);
            Stage = PairingStage.Offering;
        }
        catch (FsyncException e) { Fail(e); }
    }

    /// <summary>This device is already paired: enter the new device's code or fsync://pair link.</summary>
    [RelayCommand]
    public async Task JoinAsync()
    {
        var input = JoinInput.Trim();
        if (input.Length == 0) return;
        Reset();
        Stage = PairingStage.Joining;
        try
        {
            // Returns once the SAS is ready (both sides then confirm).
            var peer = await _bridge.RunAsync(e => e.PairingJoin(input));
            ShowSas(peer);
        }
        catch (FsyncException e) { Fail(e); }
    }

    [RelayCommand]
    public Task AcceptAsync() => Confirm(true);

    [RelayCommand]
    public Task RejectAsync() => Confirm(false);

    [RelayCommand]
    public async Task CancelAsync()
    {
        await CancelQuietly();
        Reset();
    }

    public bool IsExpired => Stage == PairingStage.Offering && ExpiresAt is { } t && _now() >= t;

    private async Task Confirm(bool accept)
    {
        if (_requestId is not { } id) return;
        try
        {
            await _bridge.RunAsync(e => e.PairingConfirm(id, accept));
            // Completed / Failed may already have arrived while the call ran: keep it.
            if (Stage == PairingStage.ConfirmSas) Stage = accept ? PairingStage.WaitingForOther : PairingStage.Idle;
        }
        catch (FsyncException e) { Fail(e); }
    }

    private void OnPairingEvent(PairingEvent ev)
    {
        switch (ev)
        {
            case PairingEvent.ConfirmRequest c:
                ShowSas(c.peer); // a newer request replaces the older id (contract)
                break;
            case PairingEvent.Completed:
                Stage = PairingStage.Completed;
                _requestId = null;
                break;
            case PairingEvent.Failed f:
                Stage = PairingStage.Failed;
                Error = f.reason;
                _requestId = null;
                break;
        }
    }

    private void ShowSas(PairingPeer peer)
    {
        _requestId = peer.requestId;
        SasWords = peer.sasWords;
        PeerName = peer.name;
        PeerPlatform = DeviceItem.PlatformName(peer.platform);
        Stage = PairingStage.ConfirmSas;
    }

    private void Fail(FsyncException e)
    {
        Stage = PairingStage.Failed;
        Error = e is FsyncException.Cancelled ? "The other device rejected the pairing." : UserMessages.For(e);
    }

    private void Reset()
    {
        (OfferCode, QrPayload, QrPng, ExpiresAt, SasWords, PeerName, PeerPlatform, Error) = (null, null, null, null, null, null, null, null);
        _requestId = null;
        Stage = PairingStage.Idle;
    }

    private async Task CancelQuietly()
    {
        if (Stage is PairingStage.Idle or PairingStage.Completed or PairingStage.Failed) return;
        try { await _bridge.RunAsync(e => e.PairingCancel()); } catch (FsyncException) { }
    }

    public void Dispose() => _bridge.PairingEventReceived -= OnPairingEvent;
}
