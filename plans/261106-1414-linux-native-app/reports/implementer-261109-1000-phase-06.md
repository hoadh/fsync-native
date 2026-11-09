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
