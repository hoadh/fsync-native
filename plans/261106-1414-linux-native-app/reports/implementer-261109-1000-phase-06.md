# Phase 06 scripts: implementer report (261109)
Status: X1 PASS locally (docs/test-results/linux-cross-os-261109-1625.json: text 38/64 ms, file 2879/2124 ms, sha256 equal, no echo/dup). Windows legs written, NOT run (no pwsh here; ps1 not parse-checked). Not committed.
Files: apps/linux/tests/e2e/{cross-os-with-windows.sh,lib-cross-os-windows-legs.sh}, apps/windows/tests/e2e/{cross-os-cli-peer.ps1,cross-os-port-forward.ps1}, docs/test-results/linux-cross-os-261109-1625.json.
OTP: not bound to client (backend otp.rs: keyed by email+code hash, IP only rate-limited, single use, TTL 5 min). Host calls /auth/otp/start, reads OTP from backend log, passes -Otp.

## Update 261109 (ref f7f8173, reverse tunnel)
X1 PASS again inside the full-mode run (tunnel up, backend/relay on 18080/18090). Build run on waso-vm FAILED: `'pwsh' is not recognized` (worker has Windows PowerShell 5.1 only; cargo present at C:\Users\dev\.cargo\bin, dotnet not on PATH, registry shell=cmd). Nothing else ran (X2/X5/pair not reached). Tunnel + health gate worked (gate passed before the run).
Fix prepared, uncommitted: win_remote uses `powershell -NoProfile -ExecutionPolicy Bypass -File`; cli-peer `Kill($true)` (PS7-only) -> Stop-Process; comments. Needs push, then rerun REF=<new sha>. Alternative: user installs pwsh 7 on waso-vm (then revert to pwsh).
