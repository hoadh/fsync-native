using Fsync.Core;

namespace Fsync.App.Bridge;

/// <summary>
/// Core callbacks arrive on the "fsync-callbacks" Rust thread. Session tokens are persisted
/// right there (must not wait for the UI); everything else is posted to the UI thread.
/// </summary>
internal sealed class EngineDelegateAdapter(IUiDispatcher ui, SecretStore secrets, EngineBridge bridge) : EngineDelegate
{
    public void OnSessionChanged(SessionTokens tokens) => secrets.SaveSession(tokens);

    public void OnAccountEvent(AccountEvent @event) => ui.Post(() => bridge.HandleAccountEvent(@event));
    public void OnDeviceStatus(Device device) => ui.Post(() => bridge.RaiseDeviceStatus(device));
    public void OnPairingEvent(PairingEvent @event) => ui.Post(() => bridge.HandlePairingEvent(@event));
    public void OnError(FsyncException error) => ui.Post(() => bridge.ReportError(error));

    public void OnRemoteClip(ClipItem item) => ui.Post(() => bridge.RaiseRemoteClip(item));
    public void OnTransfer(TransferEvent @event) => ui.Post(() => bridge.RaiseTransfer(@event));
    public void OnIncomingFiles(ClipItem offer) => ui.Post(() => bridge.RaiseIncomingFiles(offer));
}
