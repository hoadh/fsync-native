# Re-review: phase 06 (fix round 1 + docs pass)

Scope: `4bedaf1..e925d88`, at HEAD e925d88 (clean). Commits: 1e066a8 (codec), 312a67a (harness), 578ce34 (1241 evidence), e925d88 (docs). Each finding was checked against the code, not taken from the fix claims.
Static checks:
- `bash -n` passes on all 3 Linux scripts.
- shellcheck `-S warning` reports only SC2034 (unused `i`, `rows`, `ACCEPT_WAIT`) and SC1010 false positives (`done` used as an argument).
- The Windows suite (173/173) could not be run here. Its count matches the changelog.

**Score: 9.5 / 10. Critical 0, High 0, Medium 0, Low 5. APPROVED (bar 9.5, 0 critical).**

## Verification of earlier findings

| ID | Verdict | Evidence |
|---|---|---|
| H1 | Fixed | `ImageCodec.cs`: the repeated masks are skipped only when `headerSize >= 56`, the DIB is long enough, and `dib[40..40+n] SequenceEqual dib[headerSize..+n]`. Slack after the pixels no longer counts. Test `DibV5WithoutTrailingMasksButWithSlackKeepsEveryPixel` (V5, no copy of the masks, 16 bytes of slack, exact pixels). A truncation check was added too (`dib.Length - offset < stride*height`). |
| H2 | Fixed | ps1 writes `sent-`/`set-` only on try 1 and writes `tries-<it>` on every try. Linux requires `^[1-4]$`, puts `tries` in the JSON, sets `checks.retries`, and adds a "RETRY" prefix to the note. The latency is always measured from the first copy. |
| H3 | Fixed | The hardcoded field is gone. `echo_ok` records only assertions that passed. X3 out: devWG +4 exactly, L1 +0. X3 in-auto: devWG +0, L1 +3. X3 in-lazy: devWG +0. X1 is recorded. `echo_not_checked` lists X2/X5 when they run. The 1241 JSON matches. |
| H4 | Fixed | ADR 014 is accepted and has the 1241 table; 1217 is marked superseded and 1753's echo claim is qualified. The README now says 10 s, has a runs table and describes the fields. The phase file has its todos ticked, As-Built Notes and the gaps. plan.md row updated. Changelog has the codec bug and the 1241 entry. Anchors resolve. |
| M1 | Fixed | One ssh stream of about 150 samples; skew = max(t_win − t_arrival) after 20 warm-up samples; error = ss TCP RTT + 6 ms. `leg` fails below −err and gates on latency + err. `skew_ms`/`skew_err_ms` are their own fields. In 1241, the three probes agree (8051/8054/8055) and every latency is positive, which also rules out stdout buffering. |
| M2 | Fixed | The test image now has distinct R/G/B formulas on both sides (`gui_make_png`, `gui_png_ok`, `Get-TestRgb`), and `gui_png_ok` refuses grey PNGs. Alpha is untested, and the docs say so. |
| M3 | Fixed | Width and height use `Math.Abs((long)…)`; bitCount must be one of {1,4,8,16,24,32}; each side is checked against `0 < side <= MaxSide`; `offset` is now a `long`. `TryDibToPngAsync` catches everything except OOM, so text and html survive. Tests: `HostileDibHeadersAreRejectedAsInvalidData` (5 cases) and `UndecodableDibDropsOnlyTheImageNotTheText`. |
| M4 | Fixed (log only) | `AppLog.Warn("image not synced: …")`. Converting non-standard masks is not done; it is listed in the ADR "Codec gaps". |
| M5 | Fixed | `gui_abort` uses PowerShell to unregister `fsync-xos-gui-*` tasks, stops only the processes whose `Path` equals `exe-path.txt`, and removes `gui-launch-*`. I traced cmd.exe's quote toggling through the `\"` pairs: every pipe stays inside a quoted region. |
| M6 | Fixed | The launcher starts with `$code = 99`, wraps the call in try/catch, writes the code to `.tmp` and moves it into place (atomic). The wrapper requires the code to match `^\d{1,3}$` and exits 98 otherwise. |
| L1 | Fixed | `hasAlphaMask = headerSize >= 56 \|\| (40 && ALPHABITFIELDS)`. Test `V2HeaderHasNoAlphaMaskAndNoTrailingMasks`. |
| L2 | Fixed | The mask copy is now checked before the colour table is added. |
| L3 | Fixed | `Get-ScheduledTask … .State` (an enum, so locale-independent). |
| L4 | Fixed | `gui-launch-$PID` is removed in `finally` and by `gui_abort`. |
| L5 | Fixed | Task name `fsync-xos-gui-$PID`; a quote in `$PSCommandPath` is refused. |
| L7 | Fixed | BITMAPCOREHEADER: `bcBitCount` at offset 10 is validated, and the palette offset is `3 << bits`. |

## Low (new or residual, non-blocking)

1. **The ADR has one stale line.** `docs/adr/014-linux-desktop-app.md:569-570` says the `cross-os-cli-peer.ps1` header "still says PowerShell 7; that header is wrong". 312a67a already fixed that header to 5.1. Delete the bullet.

2. **The in-lazy echo check does not prove devA's watch is still running.** `gui_echo_check x3-in-lazy` expects devWG's count to be unchanged. If devA's `clip watch` had exited (`--timeout 1500` s since X1), the check would pass vacuously. In-auto proves the watch is alive a minute earlier (L1 +3), so the risk is small. Fix: `kill -0` the watch PID, or also assert L1 +1 for the file clip.

3. **A retry now always fails the run.** Each retry is a new copy, so the Windows app publishes it again. The out-leg echo check (`wg_before + 4` exactly) then fails, unless core deduplicates identical content. The behaviour is stricter, which is fine. But the docs and the note say retries are "flagged", which reads as pass-with-flag. Say "a retry fails the echo check", or count the expected publishes as `+ sum(tries)`.

4. **The skew error bound is narrower than its label suggests.** The bound is the TCP RTT; the estimate (a maximum) is one-sided and can sit low by the PowerShell-to-sshd write latency, which RTT + 6 ms does not cover. On win-to-app legs this hides at most a few ms; app-to-win legs are conservative. Not material at the 5 s and 10 s gates. Calling it a "heuristic bound" would be more accurate.

5. **Two minor carry-overs.**
   - The phase-06 As-Built Notes say "review round … fixed in 312a67a", but the codec fixes are in 1e066a8.
   - It is still unknown whether a V5 synthesized from a BI_ALPHABITFIELDS DIB repeats 3 masks or 4. The code compares 16 bytes. If Windows repeats only 3, the copy is not skipped. Rare format.

## Positive
- Every fix comes with a targeted test or an evidence field, and the hostile-header cases are covered.
- The evidence is now honest: the superseded 1217 run is labelled as such, the 1753 echo claim is qualified, and the gaps (QR, X2/X5 re-run, the 8.7 s lazy-file margin, the offline-peer delay, alpha, 16-bpp masks) are written down.
- The skew probe works: probes ±8 ms apart, no negative latencies, and the gate includes the error.
- Windows-side hygiene: kills by exact path, unique task names, atomic exit code, launch directory removed.

## Unresolved questions
- Does core or the Windows app deduplicate a repeated identical copy? This decides whether a retry fails the out-leg echo check (Low 3).
- How many masks does Windows repeat for a V5 synthesized from BI_ALPHABITFIELDS (Low 5)?
