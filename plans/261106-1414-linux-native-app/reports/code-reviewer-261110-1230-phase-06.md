# Code review: phase 06 cross-OS (X3/X4) + CF_DIBV5 fix

Scope: `git diff 823b107~3..HEAD` (HEAD 4bedaf1), 23 files, +1473/-47. Static only (no Windows here).
Checks: `bash -n` ok on the 3 Linux scripts; shellcheck (docker, -S warning): only SC2034 unused vars (`rows`, `i`, `ACCEPT_WAIT`) and SC1010 false positives (`done` as an argument). Nothing blocking.

**Score: 7.5 / 10. Critical: 0. High: 4. Not auto-approvable (bar 9.5).**

## Critical
None.

## High

H1. ImageCodec "room for masks" heuristic can misdetect, causing the same 3-pixel shift it fixes (`ImageCodec.cs:72-81`).
`dib.Length - offset >= stride*height + maskBytes` uses `>=`, so any V4/V5 BITFIELDS DIB with 12 or more spare bytes after the pixels gets its pixels shifted by 12 bytes (3 px at 32 bpp), with no error. Spare bytes do occur in real input:
- `Win32Clipboard.Read` sizes the copy with `GlobalSize`, which may be larger than the requested allocation. The code already allows for this: see the `TrimNul` comment, "HGLOBAL rounding".
- Apps that write CF_DIBV5 themselves, with masks only in the header, sometimes over-allocate.
- A V5 header with an embedded ICC profile (`bV5CSType = PROFILE_EMBEDDED`) places the profile after the bits.
Fix:
- Require the repeated masks to match the header masks: `dib[headerSize..headerSize+12]` must equal `dib[40..52]`, plus `dib[52..56]` for ALPHABITFIELDS. The chance that real pixel data matches all of these is close to zero.
- Optionally also require `bV5ProfileData`, when present, to start after `offset + stride*height`.
- Add a test: a V5 DIB with no trailing masks plus 16 bytes of slack must decode exactly. There is no negative test today. `BuildDib` always writes the trailing masks when `headerSize > 40`.

H2. The out-leg latency gate is weakened by silent retries (`cross-os-gui-peer.ps1:222-231`).
Each try overwrites `sent-<item>` with a new timestamp before it copies again (4 tries, 15 s apart). The Linux side polls for 40 s and reads `sent-` only after it sees the clip. So if the first copy is lost or takes more than 15 s, the leg still passes with roughly 1 s latency measured from the retry. The try count is not in the JSON, so the evidence cannot show whether this happened in the 1217 run. The stated reason (rdpclip re-posting with the no-history flag) is either a real product loss in RDP sessions or a test artefact. Either way it must be visible.
Fix: keep the first `sent-` (write `sent-<it>-<try>`), report `tries` in the JSON and in the leg note, and fail or flag when tries > 1. At minimum, measure latency from try 1.

H3. The `no_echo_or_duplicates: true` field is hardcoded (`cross-os-with-windows.sh:292`).
Only X1 asserts it (history counts at :268-273). The X3 legs (and X2/X5) have no echo or duplicate check: nothing checks that the Windows clipboard stayed unchanged after an app-to-win leg, and nothing counts history on either side. Even so, the 1217 JSON and the implementer report claim "no echo".
Fix: add an X3 check, for example a history count on L1 per nonce after a short settling time, and on Windows check that no extra publish appears in the app log (`clipboard: published` count) or that the clipboard sequence number did not change. Otherwise write the field as `"x1_only"` or `null` for those scopes.

H4. Docs and evidence are inconsistent after the X3/X4 pass.
- ADR 014 still says "Pending (not run): X3 … X4" (line ~519), has no X3/X4 rows in the results table, and its status is still draft.
- The phase-06 status line, its `[ ] X3 + X4` todo and the plan.md row still say pending.
- `docs/test-results/README.md` says Windows file legs are "gated at 30 s" (the harness and the ADR say 10 s). Its scope list leaves out `X1+X4+X3`. "Latest full pass" still points to 1753.
- The implementer report header says "No result JSON committed (X3 not complete)" while its last section says it is complete.
- The spec's X4 asks for "code and QR text"; only the code path is tested and nothing records this gap.
- The "taskMode restored" item has no evidence.

## Medium

M1. The skew estimate is biased, which explains the negative latencies (`lib-cross-os-windows-legs.sh:40-54`).
The midpoint `(a+b)/2` assumes the remote timestamp falls in the middle of the round trip. Here the round trip includes the ssh handshake and Windows PowerShell 5.1 start-up, both before `UtcNow`, so the timestamp falls late. The skew is therefore overestimated: app-to-win latencies come out too low and win-to-app too high. The html -180 ms and png -311 ms results are physically impossible. They are worse than they look, because `offer_ms` is taken before `clip_offer` spawns and `recv` is stamped after a 250 ms poll plus a GDI pixel check. So the bias is at least ~0.3-0.5 s. The ±541/458 ms bound covers it today, and the 5 s gate leaves a wide margin, so no failure is hidden at current numbers.
Fix:
- Sample from one long-lived `powershell` over a single ssh session (send a line, read a timestamp), which brings the round trip down to milliseconds. Or sync the VM clock (`w32tm /resync`) and assert the skew is under 50 ms.
- In `leg`, fail when `latency < -ERR`.
- Gate conservatively on `latency + ERR <= limit`.
- Record `skew_ms` and `skew_err_ms` as JSON fields, not only in the note.

M2. The test image is grey (R=G=B), so a channel-order bug cannot be detected (`gui_make_png`, `New-TestBitmap`, `Test-BitmapPixels`, `gui_png_ok`).
This is exactly the bug class `CheckStandardMasks` guards against, and alpha is never tested either. Use distinct per-channel values (for example R=x*7, G=y*3, B=x^y) on both sides.

M3. `Math.Abs(width)` throws `OverflowException` when width is `int.MinValue` (`ImageCodec.cs:78`).
Any local app can trigger this with a hostile CF_DIBV5. `TryDibToPngAsync` only catches InvalidData, Argument and COM exceptions, so the exception reaches `PublishAsync`'s catch-all and the whole clip is dropped, text and html included, with "Could not send the clipboard." Also, `stride*height` can wrap a `long` because `bitCount` is not validated (up to 65535). The result is only a wrong offset (WIC then rejects it), but it is sloppy.
Fix: validate `bitCount ∈ {1,4,8,16,24,32}` and `|width|,|height| <= MaxSide` in `DibToBmpFile`, and throw InvalidDataException.

M4. Rejecting non-standard 32-bpp masks is safe, but the image is lost silently.
`TryDibToPngAsync` returns null with no log line, so a user copying from an app that writes RGBA-ordered masks gets text-only syncs and no hint why.
- Add an `AppLog.Warn` with the masks.
- Better: remap byte-aligned 32-bpp masks into BGRA (a cheap swizzle) instead of refusing.
16-bpp BITFIELDS (565/555) is correctly not rejected. Other 16-bpp masks (for example 444) still go to WIC unchecked; whether WIC honours them is not verified.

M5. `gui_abort` runs `taskkill /F /IM Fsync.exe` (`lib-cross-os-gui-legs.sh:49`).
This kills every Fsync.exe in the shared interactive session, including a real install the dev user may be running. `Stop-GuiApps` in the ps1 is correctly scoped by path. Use the same path-scoped kill, for example `powershell … Get-Process Fsync | ? Path -eq <test exe> | Stop-Process`.

M6. The wrapper's exit code can fail open (`cross-os-gui-peer.ps1:44-45,62`).
If the inner script dies before reaching its own `exit` (parameter binding or parse error), the launcher's `$LASTEXITCODE` is `$null` and an empty `exit.txt` is written. `[int]''` is 0, so the wrapper exits 0. The same happens if `exit.txt` is read while it is still being written. The Linux side's sync-file and JSON checks catch most of these cases (pair, in-auto and in-lazy parse JSON; out needs the sync files). Still, make the launcher write `if ($null -eq $LASTEXITCODE) {99} else {$LASTEXITCODE}` and treat an empty or non-numeric code as failure.

## Low
- L1. `CheckStandardMasks` reads offset 52 as the alpha mask for any `headerSize > 40`, including a 52-byte BITMAPV2INFOHEADER, which has no alpha mask. A valid V2 DIB can then be rejected. Use `headerSize >= 56`.
- L2. When the heuristic applies to a V4/V5 DIB with `clrUsed != 0`, it adds the masks after the colour table, while the synthesized masks would come before it. This is an edge case.
- L3. `schtasks /query … -match 'Running'` only works with English status text; on a localised OS the wait loop exits after 10 s.
- L4. The OTP and email stay in `%TEMP%\fsync-cross-os\gui-launch\launch.ps1` after the run. The OTP is a single-use dev code, so this is minor. Delete the gui-launch directory in the `finally` block.
- L5. The fixed task name `fsync-xos-gui` and a `$PSCommandPath` containing `'` would break the launcher. The worker is serialised, so this does not bite in practice.
- L6. Leftover task: the `finally` deletes it, and `gui_abort` does too when the outer run is killed. This path is covered.
- L7. Pre-existing, not in this diff: a BITMAPCOREHEADER (12-byte) DIB with a palette gets no colour-table offset. Bundle the fix with M3.

## Positive
- The root cause was found from a real `GetClipboardData(17)` dump, and the regression tests use distinct channels and exact pixels. `PngToDib` is checked at byte level without using our own decoder.
- Worker output is handled as data: sync names and values are regex-validated on both sides, the pair code is checked as `^[0-9]{6}$` before it goes into the IPC JSON, and the ps1 rejects quote characters before building `argText`. No injection path found.
- The Linux clipboard work runs in a nested sway (`wl_a`, `--socket "$SA_RT/$SA_WL"`). The host's clipboard, keyring and desktop are not touched.
- No password is stored for the scheduled task (`/it /ru` with no `/rp`); the `finally` and `gui_abort` both delete the task.
- The per-leg `limit_ms` in the JSON makes the 10 s file gate explicit. Clip gates stay at 5 s and exact-pixel/sha256 checks are kept.
- The product finding is well documented: one offline peer delays the next publish by up to 7 s.

## Recommended actions (in order)
1. H1: mask-equality check in the heuristic, plus a negative test (V5 with no trailing masks and slack).
2. H2: measure latency from the first try, record `tries`, flag retries; rerun X3.
3. H3: a real X3 echo/duplicate check, or stop claiming it.
4. H4: update ADR 014 (X3/X4 rows, status), the phase-06 file, plan.md and the test-results README (10 s, scope, latest run).
5. M1/M2: a low-RTT skew probe plus a lower-bound check; colour test image.
6. M3–M6.

## Unresolved questions
- What is the default ssh shell on waso-vm? `gui_abort` chains commands with cmd `&`; under PowerShell 5.1 as the default shell the whole line fails silently because of `|| true`.
- Did any out-leg retry happen in the 1217 run? It cannot be told from the JSON or the report.
- When Windows synthesizes CF_DIBV5 from a BI_ALPHABITFIELDS CF_DIB, does it repeat 3 masks or 4? The code assumes 4.
- Is the rdpclip re-post (the reason for the retry) a real product issue for RDP users?
- X4 "QR text": was it dropped from scope deliberately?
