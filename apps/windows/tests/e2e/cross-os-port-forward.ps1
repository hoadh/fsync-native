# VERIFY-ONLY check of the reverse tunnel for the Linux cross-OS test (phase 06, Option A).
# The Linux host opens `ssh -N -R 127.0.0.1:<port>:127.0.0.1:<port> <this machine>` for the whole run
# (apps/linux/tests/e2e/cross-os-with-windows.sh), so this machine's own 127.0.0.1:<ApiPort> and
# 127.0.0.1:<RelayPort> reach the dev backend and relay on the Linux host. The Windows peer then uses
# the SAME http://127.0.0.1:<port> URLs as the Linux apps (plain http is accepted only for loopback;
# no override is added anywhere). TCP only: QUIC address discovery is not forwarded, so relay legs
# run over TCP. This script changes nothing; it only checks. PowerShell 7, idempotent.
#
#   cross-os-port-forward.ps1 -ApiPort 18080 -RelayPort 18090
# Exit 0: GET /health answers 200 and the relay port accepts a TCP connection. Output: one JSON line.
param(
    [Parameter(Mandatory)][ValidateRange(1024, 65535)][int]$ApiPort,
    [Parameter(Mandatory)][ValidateRange(1024, 65535)][int]$RelayPort
)
$ErrorActionPreference = 'Stop'
function Out-Json([hashtable]$o) { $o | ConvertTo-Json -Compress }
try {
    $health = $false
    try { $health = (Invoke-WebRequest "http://127.0.0.1:$ApiPort/health" -UseBasicParsing -TimeoutSec 5).StatusCode -eq 200 } catch {}
    $relay = $false
    $c = [System.Net.Sockets.TcpClient]::new()
    try { $relay = $c.ConnectAsync('127.0.0.1', $RelayPort).Wait(5000) -and $c.Connected } catch {} finally { $c.Dispose() }
    $ok = $health -and $relay
    Out-Json @{ leg = 'forward'; ok = $ok; api_health = $health; relay_tcp = $relay }
    exit $(if ($ok) { 0 } else { 1 })
}
catch {
    Out-Json @{ leg = 'forward'; ok = $false; error = "$_" }
    exit 1
}
