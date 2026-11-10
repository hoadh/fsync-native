# Phase 06 X3/X4 run report (COMPLETE: X4 + X3 all legs PASS in one run after the Windows DIB fix; see the final section)

Branch feat/linux-app, last pushed harness commit: see `git log` (test(e2e)/fix(e2e) commits after a4622a5). No result JSON committed (X3 not complete).

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
