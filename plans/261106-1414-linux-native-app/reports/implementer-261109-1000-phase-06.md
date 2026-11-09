# Phase 06 scripts: implementer report (261109)
Status: X1 PASS locally (docs/test-results/linux-cross-os-261109-1625.json: text 38/64 ms, file 2879/2124 ms, sha256 equal, no echo/dup). Windows legs written, NOT run (no pwsh here; ps1 not parse-checked). Not committed.
Files: apps/linux/tests/e2e/{cross-os-with-windows.sh,lib-cross-os-windows-legs.sh}, apps/windows/tests/e2e/{cross-os-cli-peer.ps1,cross-os-port-forward.ps1}, docs/test-results/linux-cross-os-261109-1625.json.
OTP: not bound to client (backend otp.rs: keyed by email+code hash, IP only rate-limited, single use, TTL 5 min). Host calls /auth/otp/start, reads OTP from backend log, passes -Otp.

## Update 261109 (ref f7f8173, reverse tunnel)
X1 PASS again inside the full-mode run (tunnel up, backend/relay on 18080/18090). Build run on waso-vm FAILED: `'pwsh' is not recognized` (worker has Windows PowerShell 5.1 only; cargo present at C:\Users\dev\.cargo\bin, dotnet not on PATH, registry shell=cmd). Nothing else ran (X2/X5/pair not reached). Tunnel + health gate worked (gate passed before the run).
Fix prepared, uncommitted: win_remote uses `powershell -NoProfile -ExecutionPolicy Bypass -File`; cli-peer `Kill($true)` (PS7-only) -> Stop-Process; comments. Needs push, then rerun REF=<new sha>. Alternative: user installs pwsh 7 on waso-vm (then revert to pwsh).

## Update 261109 (ref a5f09f8, PS 5.1 scripts)
Build run reached cargo and failed (run r261109093248-008d97): openssl-src needs `perl` (rusqlite bundled-sqlcipher-vendored-openssl); `perl` not on waso-vm PATH, Strawberry Perl absent (C:\Strawberry\perl\bin\perl.exe False). Git for Windows perl exists (C:\Program Files\Git\usr\bin\perl.exe) but msys perl is not a supported OpenSSL VC-WIN64A configurator (untested here). Documented prereq (apps/windows/README.md:21, build-windows-dll.ps1): Strawberry Perl + NASM. Tunnel/health gate OK; X1 PASS again in the run. No script change needed; X2/X5 not reached.

## Update 261109 (ref a5f09f8, Perl installed)
Build run on waso-vm PASSED (cargo + vendored OpenSSL), verify-only run PASSED ({"api_health":true,"relay_tcp":true} through the reverse tunnel, run r261109101828-d7b97c). Bug found + fixed on the Linux side: OTP start URL needs the /v1 prefix (`$API/v1/auth/otp/start`, was 404). Pair run (r261109101830-81be8c) failed in 2 s: `login failed:` with EMPTY stderr tail; cause not visible. Prepared (uncommitted): cli-peer error text now carries exit code + stderr and stdout tails; Linux side stops waiting as soon as the pair run ends. Needs push + rerun to see the login error. X2/X5 not reached.

## Update 261109 (ref 8578433)
Run 1: local flake before Windows (L1<->devB pairing "network: connection lost", the known just-signed-in joiner flake; rerun passed that stage). Run 2: Windows login WORKED (stdout `logged_in needs_pairing`), but cli-peer treated it as failed: in PS 5.1 `Process.ExitCode` is $null after `WaitForExit(ms)` (exit= empty), so `$login.Exit -ne 0`. Fix prepared (uncommitted): plain `$p.WaitForExit()` after the timed wait + [int] coercion (-2 when still null). CLI `login --code` does NOT start a new OTP (cmd_account.rs: start only when --code absent), so no OTP invalidation. X2/X5 not reached yet.

## Update 261109 (ref b29b91c)
Pair run still reports `exit=` empty after plain WaitForExit(); login itself succeeds (`logged_in needs_pairing`). Next fix prepared (uncommitted): `$null = $p.Handle` right after Start-Process (PS 5.1 handle caching), and pair accepts a `logged_in` event as login success when the exit code is unknown. Local L1<->devB pairing flake count: 1 in 5 full runs (run at 8578433: "Pairing failed: network: connection lost"); this run: 0.

## Update 261109 (ref e7956c4, handle fix in)
ExitCode fix works (login now accepted). Next failure: `pair join` on the Windows CLI -> "this device is not paired yet: run `pair offer` here and `pair join` elsewhere" (core/fsync-cli ctx.rs:73). A device of an existing account (login state needs_pairing) must OFFER; only an already paired device joins (same as devB vs L1 in X1). Redesign prepared (uncommitted): windows `-Step pair` runs `pair offer --yes` writing stdout to %TEMP%\fsync-cross-os\pair-offer.out; the host polls that file over ssh (code validated ^\d{6}$), L1 `pair-join`s, SAS equality still asserted. Pair-flake count: 1 in 6 full runs (unchanged).
