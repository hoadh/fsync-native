# Code review: offline peer publish delay (d54f8b7 test + 92e3bc0 fix)

Score 7.5/10. Critical 0, high 2, medium 4, low 4. NOT approved (bar 9.5). Persisted by the controller from the reviewer's hand-back.

## High
- H1: explicit "Send to..." (`send_files` → `publish_clip(push=true)` → `send_one`) is skipped too: a peer marked in the last 20 s fails at once with `PeerOffline`, no dial. User action never had the head-of-line problem. Fix: no skip for `opts.push`.
- H2: a returning peer LOSES clips (apps never retry per-peer failures: Windows `ClipboardService.Outgoing.cs:79`, macOS `ClipboardSync.swift:159`). Only clears: send success, refusal/reject, inbound clip stream. Unwired online signals: presence `online=true` (`sync.rs::run_presence_pump`, `service/presence.rs::note_presence`), inbound accept (`net/gate.rs::accept`), address hint (`Discovery::hint`).

## Medium
- M1: local network failure (sleep/resume, Wi-Fi roam, VPN, relay outage) marks every peer; all skipped for 20 s after recovery, clips lost.
- M2: `Instant` stops during suspend on Linux/macOS (not Windows): pre-sleep mark still fresh after wake.
- M3: concurrent publish + send_files: a stale `mark()` from an older attempt overwrites a newer `clear()`.
- M4: recovery paths untested (clear on inbound clip, returning peer, expiry re-probe, push path).

## Low
- L1: `< 1 s` wall-clock bound; raise to 2 s if it flakes.
- L2: skip reported as generic Net failure; add a debug log on first mark.
- L3: hardcoded 20 s `BACKOFF`; make it configurable for tests.
- L4: map growth negligible (paired peers only).

## Checked OK
Mutex not held across await, poison fails open; classification of PeerRefused/Invalid/NotTrusted/Closed; outbox `unoffer` still runs; dialer single-flight untouched; public API/FFI unchanged; force-relay same as direct.

## Recommendation (design)
Let `publish` return once fast peers finish or a short deadline (~1.5 s) passes; slow peers complete in a detached task (aborted on engine close). Removes the first-clip 7 s and the 20 s re-probe cost, and a returning peer gets the next clip without any mark, so H2/M1 mostly vanish. Needs care with shutdown and the meaning of `delivered` in `Published`.

## Unresolved questions
- Is 20 s acceptable when clips are lost, not retried?
- Should presence `online=false` set the mark?
- Was the X2 4.1 s Linux→Windows delay a first publish (not removed by this fix)?
