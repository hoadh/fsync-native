# Phase 03: Pairing (offer/QR, join by code, SAS confirm)

## Context Links
- docs/ffi-contract.md "Semantics": pairing roles, single offer window, 5 min TTL, stale request ids
- core/fsync-cli/src/cmd_pair.rs (reference flow); tests/e2e/p0-exit-demo.sh (B offers, A joins)

## Overview
Priority P1 · Status completed · 8h. There are two roles.
- **Offer** (this Mac is new, `.needsPairing`): show a code (or QR), wait for `ConfirmRequest`, show the SAS, confirm.
- **Join** (this Mac is `.ready`, "Add device…"): enter the code shown on the new device; `pairing_join` blocks until the SAS is ready; show the SAS, confirm.

`Completed` ends both.

## Key Insights
- Only one offer secret is open at a time, and a second offer throws `Invalid`. Switching Code↔QR therefore runs `pairingCancel` → `pairingOffer(mode)`.
- A newer ConfirmRequest invalidates the old `requestId`. The model replaces the shown peer and drops any confirmation in flight, following the trust-reset consent-binding pattern: `confirm(requestId:accept:)` takes the id that was on screen.
- `pairingJoin` can block for a long time. Run it in EngineCall. "Cancel" calls `pairingCancel`, which ends the join. A resulting `Cancelled` error is silent.
- After `Completed` on the offering side, call `ensureAccount` → `.ready`. On the joining side, reload `devices()`.

## Requirements
- PairingModel state:
  ```
  .idle
  .offering(PairingOffer, mode)
  .joining
  .sas(PairingPeer, role)
  .waitingForPeer      // after the local confirm
  .done(peerName)
  .failed(String)
  ```
- The SAS screen shows the words large, plus the peer name and platform verbatim (untrusted text) and the node id prefix.
  - The default button is "They don't match", which sends `accept: false`. "They match" sends `accept: true`.
  - There is no auto-accept path, so the SAS confirmation is mandatory.
- Offer screen:
  - Shows the code in groups and a copy button.
  - Counts down to `expiresMs`; on expiry it shows "Get a new code" (re-offer).
  - Has a QR toggle (CoreImage `CIQRCodeGenerator`, rendered as `NSImage`, interpolation `.none`).
- Join screen: the code field. Trimmed input is passed as-is; the core accepts a code or an `fsync://pair` URL.
- Closing the window or "Cancel" runs `pairingCancel` (errors ignored).
- `.failed` shows the `ErrorText` (context `.pairing`) or the `Failed{reason}`, with a "Try again" button.

## Architecture
```
PairOfferView/PairJoinView/SasConfirmView → PairingModel (@MainActor, engine)
   offer(mode) / join(code) / confirm(requestId:accept:) / cancel()  → EngineCall.run
EngineBridge+Events: .pairing(event) → pairingModel.handle(event)
   ConfirmRequest → .sas(peer,.offer) ; Completed → .done + bridge.afterPaired() ; Failed → .failed
bridge.afterPaired(): ensureAccount → phase; devices reload
SelfRemoved / TrustResetOutcome.removedNeedsPairing → phase .needsPairing → RootView shows PairOfferView
```

## Related Code Files
Create:
- `Sources/FsyncMacCore/Features/Pairing/PairingModel.swift`
- `Sources/FsyncMacUI/Pairing/PairOfferView.swift`
- `Sources/FsyncMacUI/Pairing/PairJoinView.swift`
- `Sources/FsyncMacUI/Pairing/SasConfirmView.swift`
- `Sources/FsyncMacUI/Pairing/QrCodeImage.swift`
- `Tests/FsyncMacCoreTests/PairingModelTests.swift`

Modify:
- `EngineBridge+Events.swift`: pairing routing, `afterPaired`
- `RootView.swift`: `.needsPairing` shows the offer flow; "Add device" shows the join flow

## FFI calls
`pairingOffer(mode:)`, `pairingJoin(input:)`, `pairingConfirm(requestId:accept:)`, `pairingCancel()`, delegate `onPairingEvent`, `ensureAccount`, `devices`.

## Implementation Steps
1. PairingModel and the event handler as a pure state transition (`func handle(_ e: PairingEvent)`), unit tested with real `PairingEvent`/`PairingPeer` values.
2. Offer/join/confirm/cancel through EngineCall. A busy guard rejects stale ids locally: if the id is no longer the shown peer, nothing is sent.
3. QrCodeImage helper (pure function: string → CGImage?), tested to be non-nil and of square size.
4. Views. The SAS view has the "don't match" button as the default keyboard action.
5. Bridge routing: `afterPaired` and a "pairing finished" banner.
6. Tests against a real engine with no backend:
   - Offer without a session → error text, state `.failed`.
   - `cancel()` while idle does not throw to the UI.
   - A second ConfirmRequest replaces the shown peer, and confirm with the old id is a no-op.
   - `Completed` → `.done`.

## Todo List
- [x] PairingModel state machine + tests
- [x] Offer (code + QR + expiry) view
- [x] Join view + cancel
- [x] SAS confirm view
- [x] Bridge routing (ConfirmRequest, Completed, Failed, SelfRemoved → needsPairing)

## Success Criteria
- Unit tests green. Integration (phase 6): CLI offers and the Mac joins; then the Mac offers (needsPairing) and the CLI joins. In both runs the SAS words equal the CLI `sas` event and both sides report paired.

## Risk Assessment
| Risk | L×I | Mitigation |
|---|---|---|
| User taps "match" on a stale peer | L×H | id-bound confirm; peer change resets the view |
| Offer window left open after the window closes | M×L | `onDisappear` → cancel; the core TTL of 5 min is the backstop |
| Join blocks forever on a bad network | M×M | Cancel button → `pairingCancel`; spinner with elapsed time |

## Security Considerations
- The SAS is mandatory on both roles, with the reject button as default. Peer names are rendered as plain `Text(verbatim:)`.
- The code is shown only in the main window, not in notifications or logs.

## Next Steps
Phase 4: device list and menu.
