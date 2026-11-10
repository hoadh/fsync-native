# Phase 06 X3/X4 run report (COMPLETE after fix round 1: X4 + X3 all legs PASS in one run; see the last two sections)

Branch feat/linux-app, see `git log`. Result JSONs: docs/test-results/linux-cross-os-261110-1217.json (first pass, before review) and -1241.json (after fix round 1, the one to cite). The earlier sections below are the history of the first, partial run.

## Result so far
- X4 PASS (3 runs): Windows GUI app signs in via UIA, offers a code, Linux app joins, SAS words equal on both sides, "Paired" shown,
  Devices row `devL1, Command line, Online, ...` (Windows labels Platform.Cli "Command line").
- X3 win->app: text 568-605 ms, html 573 ms (once the offline-peer artifact below is removed). png: Linux receives bytes but pixels differ (below).
- X3 app->win (in-auto, in-lazy): not reached yet (win->app out run fails at png first).

## Harness fixes made (all pushed)
1. `remote run` executes over ssh in SESSION 0: the WinUI app crashed at start (Microsoft.UI.Xaml 0xc000027b / E_INVALIDARG, WER events) and UIA found no window.
   `cross-os-gui-peer.ps1 -Step pair|legs` now re-runs itself as a one-shot scheduled task `/it /ru <user>` in the logged-on desktop session (session 3),
   relays output + exit code. build-app stays in session 0. (Rig taskMode interactive does not change this for `remote run`; only for `dispatch`.)
2. Devices-row platform text is "Command line", not "Cli".
3. Linux harness: `gui_abort` in the EXIT trap ends the interactive task + app (a failed run otherwise kept the worker "busy" for minutes).
   Wrapper stops waiting once its task is gone. `GUI_ITEMS` (order of win->app items), `APP_LOG` (app log filter), per-leg diagnostics in the note
   (clipboard write ms, send time on host clock, when text/html first offered on L1).
4. gui_prepare keeps devB online (`clip watch`) like devA. WHY: every account device is a publish target for the Windows app and its publishes run one at a
   time (ClipboardService "one at a time, newest wins"). An OFFLINE peer costs one publish up to 2 s + 5 s (clip/send.rs FIRST_ATTEMPT + send_timeout),
   so the NEXT clip always arrived ~5.2 s late (kind-independent: html-first run gave html 646 ms, text 5256 ms; app.log: `published ... delivered=2 failed=1`).
   With devB online: `delivered=3 failed=0`, second clip 573 ms. Gates unchanged (clips 5 s, files 10 s).
   Product observation (not a harness bug): one unreachable peer delays every following clip by up to 7 s.

## Blocking finding: png win->app pixels differ (looks like a Windows app bug)
Harness copies a 40x40 gray png via `Clipboard.SetImage` (pixel (x*7+y*3)%256) on the Windows side; the app publishes (`delivered=3`), the Linux app serves
`image/png` 210 bytes (sRGB+gAMA+pHYs chunks = WIC encoder output). Decoded with ImageMagick the pixels are WRONG, shifted by 3 pixels (12 bytes):
row 0 = `6,13,20,0,7,14,21,28` instead of `0,7,14,21,28,35,...` (output x>=3 shows source x-3; x 0..2 show other data).
- Not the harness/Linux: .NET `SetImage` -> `GetImage` round trip on the VM is exact (0,7,14,21,28,35). Linux just serves the received bytes.
- The CF_DIB written by SetImage on the VM: hdr=40, bpp=32, comp=3 (BI_BITFIELDS), 12 mask bytes follow (0xFF0000,0xFF00,0xFF), len 6452, bottom-up, first pixel row = source row 39.
- A BMP built as `ImageCodec.DibToBmpFile` builds it (bfOffBits = 14+40+12) from a synthetic DIB of the same layout decodes CORRECTLY in ImageMagick.
  So the BMP file is valid; the shift appears in the WinRT BitmapDecoder (BMP) -> SoftwareBitmap -> PNG path in `apps/windows/src/Fsync.Interop/Imaging/ImageCodec.cs`
  (`DibToPngAsync`) for 32-bpp BI_BITFIELDS 40-byte-header DIBs. Likely candidates: WIC BMP decoder with a 40-byte BITFIELDS header (it may read masks as part of the
  pixel area / ignore bfOffBits), premultiplied alpha mode. Not proven which API call.
- Not weakened: the test keeps the exact-pixel check. Needs a decision from the owner of apps/windows (fix DibToPngAsync, e.g. normalise to a V5 header / strip masks,
  or prefer a "PNG" clipboard format) and a unit test with a BI_BITFIELDS-40 DIB (existing tests only check bfOffBits, ImageCodecTests.cs:70).

## Run hygiene
Worker left clean (no Fsync.exe, scheduled tasks deleted, my scratch scripts removed). Runs cancelled via `remote cancel` where a stale run held the worker.

## Unresolved questions
- Fix owner for the DIB->PNG shift? Once fixed (new REF pushed and Windows build-app rerun, ~minutes incremental), rerun
  `X3=1 REF=<sha> apps/linux/tests/e2e/cross-os-with-windows.sh` (SKIP_BUILD=1 SKIP_WIN_BUILD=1 only if neither side changed; the Windows side needs build-app after an app fix).
- Untested yet: in-auto png (Linux->Windows, expects the same exactness through PngToDibAsync), in-lazy file (Win+Alt+V, PasteHelper), and `file` win->app.
  UIA names for those paths (Save/Saved., manual-mode toggle, Win+Alt+V) are still unverified on the real UI.
- Should an offline peer delaying consecutive publishes by up to 7 s be a product issue? (Observed 5.2 s extra on clip #2.)

## Final run (2026-11-10): PASS
Result: docs/test-results/linux-cross-os-261110-1217.json, REF ea954a6 (X1 + X4 + X3, one run, exit 0).
- X3 win->app: text 956 ms, html 704, png 833 (exact pixels), file 1791 (limit 10000). app->win: text 787, html/png (negative after skew
  correction, inside the +-458/331 ms skew error: effectively instant), file via Win+Alt+V lazy 4577 ms (limit 10000). sha256/pixels equal, no echo.
- Gates unchanged (clips 5 s, files 10 s).

### Root cause of the png shift (the earlier suspicion was close but not exact)
`ClipboardReader` reads CF_DIBV5 first. Windows synthesizes it from SetImage's BI_BITFIELDS 40-byte CF_DIB as: 124-byte V5 header (compression=3,
masks inside the header) FOLLOWED BY 12 mask bytes again, then pixels (len 6536 = 124+12+6400; dumped on waso-vm via GetClipboardData(17)).
`DibToBmpFile` added the +12 only for 40-byte headers, so bfOffBits pointed 12 bytes (3 px) early. A BI_BITFIELDS-40 DIB itself decodes exactly
(that test passed before the fix, so it was not the culprit).
Fix (ImageCodec.DibToBmpFile): for V4/V5 + BITFIELDS/ALPHABITFIELDS add the mask bytes when the DIB has room for them besides the pixels
(stride*height); 32-bpp BITFIELDS with non-standard masks (not BGR 00FF0000/0000FF00/000000FF, alpha 0 or FF000000) throw InvalidDataException.
Tests (ImageCodecTests, 7 new): BITFIELDS-40, V5+trailing masks, BI_RGB 32, 24 bpp, non-standard masks rejected, PngToDib bytes (plain bottom-up
32 bpp, exact pixels read without our decoder). Before the fix: V5-trailing and masks-rejected FAILED (pixels 27,232,178,255 vs 0,1,255,255). After: full suite
164/164 (CoreBindings 9, Interop 53, App 102) on waso-vm.
PngToDibAsync (Linux->Windows) was verified by the new byte-level test and by the app->win png leg (exact pixels at the Windows clipboard).
VM left clean (no Fsync.exe, no scheduled tasks, scratch scripts deleted).

## Fix round 1 (code review 7.5, 4 high): all re-verified against the code, none pushed back
Run: docs/test-results/linux-cross-os-261110-1241.json, REF 312a67a (X1+X4+X3, one run, exit 0). Windows suite 173/173 on waso-vm (CoreBindings 9, Interop 61, App 103).
- H1 (valid): `>=` room heuristic could shift pixels 3 px on slack bytes. Now the repeated masks are skipped only when `dib[headerSize..+12/16]` equal the header masks
  (V3+ headers), and truncated pixel data is rejected. New tests: V5 without trailing masks + 16 slack bytes decodes exactly (fails with the old heuristic), V2 header
  (52 bytes, no alpha mask, L1), truncated pixels. L2: trailing masks are checked straight after the header, before the colour table. L7: BITMAPCOREHEADER palette offset.
- M3: bit count validated (1/4/8/16/24/32), |w|,|h| <= MaxSide via long (int.MinValue no longer overflows), stride arithmetic cannot overflow. Hostile-header theory test.
  `RepresentationMapper.TryDibToPngAsync` now catches any non-OOM exception, so a codec failure can never drop text/html (test: hostile DIB + text keeps text).
- M4: `AppLog.Warn("image not synced: ...")` on every drop (masks included). Swizzle for non-standard masks NOT done (optional); 16-bpp non-565/555 masks still go to WIC unchecked.
- H2 (valid): retries rewrote `sent-`. Now `sent-`/`set-` keep the FIRST try, `tries-<item>` is recorded; JSON legs carry `tries`, a retry puts `RETRY: n tries` in the note,
  `checks.retries` counts them. This run: 0 retries.
- H3 (valid): real checks, only recorded when they passed (run aborts otherwise). X3 out: devA saw exactly +4 clips from devWG, the Linux app re-published 0, one L1 history item for the
  received text. In-auto: devWG published 0 new clips (no echo), L1 published exactly 3 for 3 copies. In-lazy: devWG published 0 new clips. JSON: `checks.echo_checks` per scope,
  `echo_checked_scopes`, `echo_not_checked` (x2/x5 when run without X3: they have no echo check, recorded as such instead of hardcoded true).
- M1 (valid): the per-sample ssh probe had the handshake + PowerShell start-up inside each round trip. Now one ssh session streams the VM clock every 20 ms, stamped on arrival;
  skew = max(t_win - t_arrival) over ~130 samples, error = the connection's TCP rtt (ss) + 6 ms. Skew measured 8051 +-8 ms (was +-541). `skew_ms`/`skew_err_ms` are JSON fields per leg;
  `leg` fails when latency < -err and gates on latency + err <= limit (limits unchanged). No negative latencies in this run. An stdin-driven ping-pong was tried first and dropped:
  Windows OpenSSH + PowerShell delivered only the first stdin line.
- M2 (valid): coloured png R=(x*7+y*3) G=(x*5+y*9+31) B=(x*11+y*13+97), both sides (grey rejected by the verifier). No alpha test: CF_DIB has none, the product path drops it.
- M5: `gui_abort` kills only the Fsync.exe whose Path equals the exact path the run recorded in `%TEMP%\fsync-cross-os\gui\exe-path.txt` (cmd.exe default shell confirmed). M6: launcher writes
  exit code 99 when `$LASTEXITCODE` is null, atomically (tmp + move); wrapper exits 98 on a missing/non-numeric code. L3: task state via `Get-ScheduledTask` (locale independent).
  L4: the per-run launch dir (OTP) is removed in `finally` and by the abort (a stale pre-fix `gui-launch` dir on the VM was deleted). L5: unique task name `fsync-xos-gui-<pid>`, quote check on the script path.
- Commit note: the H1 fix and its tests are in one commit (1e066a8 `fix(windows)`), not split test-first; the negative test fails against the old `>=` rule by construction (not re-run against the old code).
- Left for the docs pass (H4): ADR 014, phase-06 file, plan.md, test-results README; the "taskMode restored" evidence; X4 QR-text gap.
- VM clean: no Fsync.exe, no fsync scheduled tasks. Foreign `work-*` dirs in %TEMP%\fsync-cross-os are older run leftovers, untouched.

## Observations
- Lazy file app->win took 8701 ms in this run (4577 ms before) against the 10 s gate: variance is large, the gate is thin. Worth a look (Win+Alt+V poll 1.5 s steps + PasteHelper), not changed.
- The sync clock probe needs `ss` and `getent` on the Linux host.

## X2/X5 re-run at 30a6f3c (CLI legs, current harness)
`X5=1 REF=30a6f3c... cross-os-with-windows.sh`: PASS on attempt 3, docs/test-results/linux-cross-os-261110-1511.json. Attempts 1 and 2 failed in the LOCAL L1<->devA pairing
("Pairing failed: network: connection lost", before any Windows leg): the known ADR 014 pairing flake, no harness or Windows cause (only docs commits since 312a67a). 2 flakes in 3 runs is
above the ADR's ~1 in 12, worth watching. Legs: x2 text app->win 4132 ms (gate 5000, close), win->app 66; x2 file 5290/3279 (gate 10000); x5 text 3126/90, x5 file 3487/4404. Skew +-8 ms. No echo
checks for X2/X5 (`echo_not_checked`). VM left clean.
