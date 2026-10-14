# Windows app end-to-end (phase 13): the WinUI app, driven over UI Automation, signs in and
# pairs with an fsync-cli device on the same machine against a local dev backend.
#
#   apps/windows/tests/e2e/pair-with-cli.ps1 [-App <Fsync.exe>] [-Relay <url>] [-Keep]
#
# Steps: Postgres (docker) + backend (dev mode, OTP in its log) -> CLI "devA" creates the account
# -> app signs in with the OTP (needs pairing) -> app shows a code -> CLI joins -> SAS words match
# on both -> app accepts -> app lists devA as connected while devA runs (<= 5 s) -> app removes devA
# -> startup toggle writes and removes the HKCU Run value. Needs: docker, release fsync-backend and
# fsync-cli (cargo build --release -p fsync-backend -p fsync-cli), the app built for x64.
param(
    [string]$App = (Join-Path $PSScriptRoot '..\..\src\Fsync.App\bin\x64\Release\net8.0-windows10.0.19041.0\win-x64\Fsync.exe'),
    [string]$Relay = '',
    [switch]$Keep
)
$ErrorActionPreference = 'Continue'
Import-Module (Join-Path $PSScriptRoot 'Uia.psm1') -Force

$Root = Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')
$Bin = if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR 'release' } else { Join-Path $Root 'target\release' }
$Cli = Join-Path $Bin 'fsync-cli.exe'
$Backend = Join-Path $Bin 'fsync-backend.exe'
foreach ($b in @($App, $Cli, $Backend)) { if (-not (Test-Path $b)) { throw "missing $b" } }

$Work = Join-Path $env:TEMP ("fsync-win-e2e-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory $Work -ErrorAction Stop | Out-Null
$Pg = "fsync-win-e2e-pg-$PID"
$procs = @()
$results = @()
function Log($m) { Write-Host "[win-e2e] $m" }
function Pass($m) { $script:results += "PASS $m"; Log "PASS $m" }
function FreePort { $l = [System.Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0); $l.Start(); $p = $l.LocalEndpoint.Port; $l.Stop(); $p }

try {
    Log 'postgres'
    docker run -d --name $Pg -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=fsync -p 127.0.0.1::5432 postgres:17 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'docker run failed' }
    $pgPort = ((docker port $Pg 5432/tcp) | Select-Object -First 1) -replace '.*:', ''
    Wait-Until -Seconds 60 -What 'postgres' { docker exec $Pg pg_isready -U postgres -d fsync 2>$null | Out-Null; $LASTEXITCODE -eq 0 } | Out-Null
    Start-Sleep 1

    Log 'backend (dev mode)'
    $apiPort = FreePort
    $Api = "http://127.0.0.1:$apiPort"
    $env:DATABASE_URL = "postgres://postgres:e2e@127.0.0.1:$pgPort/fsync"
    $env:BIND_ADDR = "127.0.0.1:$apiPort"; $env:FSYNC_DEV_MODE = '1'; $env:NO_COLOR = '1'
    $env:RUST_LOG = 'fsync_backend=debug,info'
    $backendLog = Join-Path $Work 'backend.log'
    $procs += Start-Process $Backend -PassThru -WindowStyle Hidden -RedirectStandardOutput $backendLog -RedirectStandardError (Join-Path $Work 'backend.err')
    Wait-Until -Seconds 30 -What 'backend health' { try { (Invoke-WebRequest "$Api/health" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200 } catch { $false } } | Out-Null

    function Otp([string]$email) {
        Wait-Until -Seconds 15 -What "OTP for $email" {
            $lines = (Get-Content $backendLog, (Join-Path $Work 'backend.err') -ErrorAction SilentlyContinue) -match 'dev otp' -match [regex]::Escape("to=""$email""")
            if ($lines) { ([regex]::Match($lines[-1], 'code="(\d+)"')).Groups[1].Value }
        }
    }
    $relayArgs = if ($Relay) { @('--relay', $Relay) } else { @() }
    # Not "Cli": that is a built-in alias (Clear-Item) and aliases win over functions.
    function Invoke-FsyncCli([string[]]$a) { & $Cli --data (Join-Path $Work 'A') --api $Api --insecure-http --name devA @relayArgs @a }

    $email = "win-e2e-$PID@example.test"
    Log 'CLI devA creates the account'
    Invoke-FsyncCli @('login', '--email', $email, '--send-only') | Out-Null
    $login = Invoke-FsyncCli @('login', '--email', $email, '--code', (Otp $email))
    if ($login -notmatch '"account":"created"') { throw "devA login: $login" }
    Pass 'devA created the account'

    Log 'app signs in'
    $env:FSYNC_DATA_DIR = Join-Path $Work 'win'; $env:FSYNC_API = $Api; $env:FSYNC_DEVICE_NAME = 'Win e2e'
    $env:FSYNC_RELAY = $Relay
    # Not $app: variables are case-insensitive and $App is the [string] parameter.
    $appProc = Start-Process $App -PassThru
    $procs += $appProc
    $w = Get-AppWindow $appProc.Id 60
    Set-Text $w 'Email' $email
    Invoke-Name $w 'Send code'
    Start-Sleep -Milliseconds 500
    Set-Text $w 'Code' (Otp $email)
    Invoke-Name $w 'Sign in'
    Wait-Name $w 'Show a code' 30 | Out-Null
    Pass 'app signed in with the OTP and needs pairing'

    Log 'pairing: app offers, devA joins'
    Invoke-Name $w 'Show a code'
    # Pairing codes are 6 digits (backend random_digits6).
    $code = Wait-Until -Seconds 30 -What 'pairing code on screen' { Get-Names $w | Where-Object { $_ -match '^\d{6}$' } | Select-Object -First 1 }
    $joinOut = Join-Path $Work 'join.out'
    $join = Start-Process $Cli -PassThru -WindowStyle Hidden -RedirectStandardOutput $joinOut -RedirectStandardError (Join-Path $Work 'join.err') `
        -ArgumentList (@('--data', (Join-Path $Work 'A'), '--api', $Api, '--insecure-http', '--name', 'devA') + $relayArgs + @('pair', 'join', $code, '--yes'))
    $procs += $join
    Wait-Name $w 'The words match' 60 | Out-Null
    $sasCli = Wait-Until -Seconds 30 -What 'CLI SAS' { @(Get-Content $joinOut -ErrorAction SilentlyContinue) -match '"event":"sas"' | ForEach-Object { ($_ | ConvertFrom-Json).words } | Select-Object -First 1 }
    if (-not (Find-Name $w $sasCli)) { throw "SAS differs: CLI '$sasCli', app shows $((Get-Names $w) -join ' | ')" }
    Pass "SAS words match on both sides ($sasCli)"
    Invoke-Name $w 'The words match'
    Wait-Name $w 'Paired' 60 | Out-Null
    if (-not $join.WaitForExit(30000) -or -not (@(Get-Content $joinOut) -match '"event":"paired"')) { throw "devA did not report paired: $(Get-Content $joinOut)" }
    Pass 'both sides report paired'

    Log 'device list and live status'
    Invoke-Name $w 'Devices'
    Wait-Name $w 'devA' 30 | Out-Null
    # "Online" comes from backend presence; Lan/Relay only while a QUIC connection exists (core does
    # not dial idle peers). Long-lived "Connected" needs the app to publish (phase 14).
    # No spaces in -ArgumentList items: PS 5.1 Start-Process does not quote them.
    $devA = @('--data', (Join-Path $Work 'A'), '--api', $Api, '--insecure-http', '--name', 'devA') + $relayArgs
    $watchOut = Join-Path $Work 'watch.out'
    $watch = Start-Process $Cli -PassThru -WindowStyle Hidden -RedirectStandardOutput $watchOut -RedirectStandardError (Join-Path $Work 'watch.err') `
        -ArgumentList ($devA + @('clip', 'watch', '--json', '--timeout', '120'))
    $procs += $watch
    Wait-Until -Seconds 60 -What 'devA watch ready' { @(Get-Content $watchOut -ErrorAction SilentlyContinue) -match '"event":"ready"' } | Out-Null
    $readyAt = Get-Date
    $online = Wait-Like $w '*devA*Online*' 30
    Pass ("live update: '{0}' {1:N1} s after devA was up (presence event, no refresh)" -f $online.Current.Name, ((Get-Date) - $readyAt).TotalSeconds)
    $watch.Kill()
    $goneAt = Get-Date
    $offline = Wait-Like $w '*devA*Offline*' 90
    Pass ("live update: '{0}' {1:N1} s after devA stopped" -f $offline.Current.Name, ((Get-Date) - $goneAt).TotalSeconds)

    $putOut = Join-Path $Work 'put.out'
    $put = Start-Process $Cli -PassThru -WindowStyle Hidden -RedirectStandardOutput $putOut -RedirectStandardError (Join-Path $Work 'put.err') `
        -ArgumentList ($devA + @('clip', 'put', '--text', 'hello-from-devA', '--wait', '60'))
    $procs += $put
    if (-not $put.WaitForExit(90000)) { throw 'devA clip put did not finish' }
    $published = @(Get-Content $putOut) -match '"event":"published"' | Select-Object -Last 1
    if (-not $published -or ($published | ConvertFrom-Json).delivered.Count -lt 1) { throw "clip not delivered to the app: $(Get-Content $putOut) $(Get-Content (Join-Path $Work 'put.err'))" }
    Pass "devA -> app clip delivered over QUIC (app engine acked): $published"

    Log 'remove devA'
    Invoke-Name $w 'Remove'
    Invoke-Name $w 'Remove' 10 # the dialog's primary button (the row button is behind the dialog)
    Wait-Until -Seconds 30 -What 'devA gone' { -not (Find-Name $w 'devA') } | Out-Null
    Pass 'devA removed from the list'

    Log 'start with Windows'
    Invoke-Name $w 'Settings'
    Invoke-Name $w 'Start fsync when I sign in to Windows'
    $run = Wait-Until -Seconds 10 -What 'Run value' { (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Fsync }
    Invoke-Name $w 'Start fsync when I sign in to Windows'
    Wait-Until -Seconds 10 -What 'Run value removed' { -not (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Fsync } | Out-Null
    Pass "startup toggle wrote '$run' and removed it"
}
catch {
    $results += "FAIL $_"
    Log "FAIL $_"
    if ($w) { try { Log ("screen: " + ((Get-Names $w) -join " | ")) } catch {} }
    Log "logs in $Work"
    $Keep = $true
}
finally {
    foreach ($p in $procs) { if ($p -and -not $p.HasExited) { try { $p.Kill() } catch {} } }
    docker rm -f -v $Pg 2>$null | Out-Null
    $results | Set-Content (Join-Path $Work 'results.txt')
    if (-not $Keep) { Start-Sleep 1; Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue }
}
$results
if ($results -match '^FAIL') { exit 1 }
