# N3 Relay CA and QAD, PeerRefused Surfaced, and the Windows Clipboard Retry

**Date**: 2026-10-17 18:30
**Severity**: High
**Component**: tests/e2e (netns N3/N4), fsync-core (relay test hook, clip/2 errors), FFI `Published`, fsync-windows clipboard writer
**Status**: Ongoing. Merged to local main c429406, not pushed. Prod relay QAD still off. Refusal text and ARM64 DLL open.

## What Happened

Owner asked three things: a test hook for N3 (relay CA + QAD), whether the app must advertise the relay URL early, and whether the UI needs a separate FFI `Refused` error. Owner also signed off Track K second-fetch timing. Three tracks landed on main: N3 test hook, Track R (`FsyncError::PeerRefused`), and a Windows clipboard fix found during verification.

Final gates on merged main: `cargo test` x2 360 pass / 0 fail, clippy clean including `test-relay-ca`, xcframework and release guard OK, FsyncCoreKit 14/0, FsyncMac 231/0 (18 skipped), p0 PASS, netns strict PASS=10 FAIL=0 SKIP=1 (`docs/test-results/netns-261017-1821.json`), Windows dotnet 158/158.

## The Brutal Truth

The ~4 s first connect that N4 flagged was not our dial. It was iroh's net report waiting about 3 s for QAD from a relay that did not speak QAD. The dial itself took 5-14 ms. We were ready to ship a "fix" that advertised the relay URL early. That would have hidden the symptom and sent peers to a relay we were not yet on.

The embarrassing one: my first release-build guard was fail-open. `cargo tree -e normal,build,features | grep test-relay-ca` never matched, because the default tree format hides enabled features. The guard passed while the feature was on. A grep that finds nothing is not a check.

The Windows bug was real and only showed up in an RDP session. `rdpclip.exe` opens the clipboard after each change, and our OLE `ClearIfCurrent` and `Flush` did not retry `CLIPBRD_E_CANT_OPEN` (0x800401D0). A reviewer said `Thread.Sleep` pumps messages on an STA thread, so the retry would be fine. I did not trust that. A 10-line probe showed it does not pump. The DoEvents control case proved the probe was sound.

## Technical Details

- N3 hook: feature `test-relay-ca`, env `FSYNC_TEST_RELAY_CA`. Private-CA TLS handshake test. Release builds fail if the feature is present.
- Guard fix: `cargo tree -e normal,build,features --format '{p} {f}'`, output captured, cargo failure aborts. Verified with a positive and negative control.
- Track R: `FsyncError::PeerRefused`, `Published.refused`. Mac/Win text: "That device removed this Mac/PC. Pair again to send to it." Review 9.5.
- n8 e2e still grepped the old `NotPaired` text. That is a deterministic strict failure, fixed in c429406.
- Clipboard: shared `Retry` (10 x 25 ms). Ownership is re-checked on every clear attempt, otherwise we could wipe a user's new copy. Reader budget aligned to 250 ms. Review 8 to 9 to 9.5.

## What We Tried

- Early relay-URL advertising: rejected. Symptom fix, and it dials a relay we are not on yet.
- Tuning N4 timeouts: rejected. Would have hidden the QAD wait.
- Trusting the release guard exit code: rejected after the fail-open finding.

## Root Cause Analysis

The latency was a missing QAD response from a relay we run without QAD, not slow connects. The guard bug was a shell check that could not fail on the input it actually received. The clipboard bug was a retry that covered the OLE write path but missed the reader race that real RDP sessions trigger.

## Lessons Learned

- Measure where the time goes before adding an advertisement or timeout.
- A guard needs a negative control, or it proves nothing.
- Do not accept "STA sleep pumps messages" from a review without a probe.
- Clipboard writes need retry on `CLIPBRD_E_CANT_OPEN`, and ownership must be checked before each retry.

## Next Steps

- **Owner**: decide whether production relays enable `RELAY_QAD` (currently 0). Needs UDP 7842 and cert access.
- **App owners**: apps do not call `send_files` or read `Published.refused` yet. Wire them before claiming the refusal feature works.
- **Product**: the refusal text does not name the device. Decide whether to add a device name.
- **Windows owner**: ARM64 DLL build is not verified.
- **Lead**: decide when to push c429406. It is local only.

**Status:** DONE_WITH_CONCERNS
**Summary:** N3 hook, PeerRefused surfacing, and Windows clipboard retry are merged locally to c429406. All gates green, netns strict 10/0/1.
**Concerns:** Prod QAD off, app-side refusal handling not wired, refusal text is generic, ARM64 unverified.
