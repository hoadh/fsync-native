# Windows app end-to-end (phase 13): the WinUI app, driven over UI Automation, signs in and
# pairs with an fsync-cli device on the same machine against a local dev backend.
#
#   apps/windows/tests/e2e/pair-with-cli.ps1 [-App <Fsync.exe>] [-Relay <url>] [-Keep]
#
# Steps: Postgres (docker, or FSYNC_E2E_PG_BIN) + local relay + backend (dev mode, OTP in its log)
# -> CLI "devA" creates the account -> app signs in with the OTP (needs pairing) -> app shows a
# code -> CLI joins -> SAS words match on both -> app accepts -> app lists devA online / offline
# from presence -> a devA clip reaches the app -> app removes devA -> startup toggle writes and
# removes the HKCU Run value. Needs an interactive desktop, release fsync-backend and fsync-cli
# (cargo build --release -p fsync-backend -p fsync-cli), the relay spike, the app built for x64.
param(
    [string]$App = (Join-Path $PSScriptRoot '..\..\src\Fsync.App\bin\x64\Release\net8.0-windows10.0.19041.0\win-x64\Fsync.exe'),
    [string]$Relay = '',
    [switch]$Keep
)
$ErrorActionPreference = 'Continue'
Import-Module (Join-Path $PSScriptRoot 'E2eHost.psm1') -Force
if (-not (Test-Path $App)) { throw "missing $App" }
$c = New-E2eContext (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')) $Relay
$w = $null

try {
    Start-E2eBackend $c
    $email = "win-e2e-$PID@example.test"
    New-CliAccount $c 'devA' $email
    Add-Pass $c 'devA created the account'

    $appProc = Start-FsyncApp $c $App 'W1'
    $w = Get-AppWindow $appProc.Id 60
    Set-Text $w 'Email' $email
    Invoke-Name $w 'Send code'
    Start-Sleep -Milliseconds 500
    Set-Text $w 'Code' (Get-Otp $c $email)
    Invoke-Name $w 'Sign in'
    Wait-Name $w 'Show a code' 30 | Out-Null
    Add-Pass $c 'app signed in with the OTP and needs pairing'

    Invoke-Name $w 'Show a code'
    # Pairing codes are 6 digits (backend random_digits6).
    $code = Wait-Until -Seconds 30 -What 'pairing code on screen' { Get-Names $w | Where-Object { $_ -match '^\d{6}$' } | Select-Object -First 1 }
    $join = Start-Cli $c 'devA' 'join' @('pair', 'join', $code, '--yes')
    Wait-Name $w 'The words match' 60 | Out-Null
    $sasCli = Wait-Until -Seconds 30 -What 'CLI SAS' { Get-CliEvents $join.Out 'sas' | ForEach-Object { $_.words } | Select-Object -First 1 }
    if (-not (Find-Name $w $sasCli)) { throw "SAS differs: CLI '$sasCli', app shows $((Get-Names $w) -join ' | ')" }
    Add-Pass $c "SAS words match on both sides ($sasCli)"
    Invoke-Name $w 'The words match'
    Wait-Name $w 'Paired' 60 | Out-Null
    if (-not $join.P.WaitForExit(30000) -or -not (Get-CliEvents $join.Out 'paired')) { throw 'devA did not report paired' }
    Add-Pass $c 'both sides report paired'

    Invoke-Name $w 'Devices'
    Wait-Name $w 'devA' 30 | Out-Null
    # "Online" comes from backend presence; Lan/Relay only while a QUIC connection exists.
    $watch = Start-Cli $c 'devA' 'watch' @('clip', 'watch', '--json', '--timeout', '120')
    Wait-Until -Seconds 60 -What 'devA watch ready' { Get-CliEvents $watch.Out 'ready' } | Out-Null
    $readyAt = Get-Date
    $online = Wait-Like $w '*devA*Online*' 30
    Add-Pass $c ("live update: '{0}' {1:N1} s after devA was up (presence event, no refresh)" -f $online.Current.Name, ((Get-Date) - $readyAt).TotalSeconds)
    $watch.P.Kill()
    $goneAt = Get-Date
    $offline = Wait-Like $w '*devA*Offline*' 90
    Add-Pass $c ("live update: '{0}' {1:N1} s after devA stopped" -f $offline.Current.Name, ((Get-Date) - $goneAt).TotalSeconds)

    $put = Start-Cli $c 'devA' 'put' @('clip', 'put', '--text', 'hello-from-devA', '--wait', '60')
    if (-not $put.P.WaitForExit(90000)) { throw 'devA clip put did not finish' }
    $published = Get-CliEvents $put.Out 'published' | Select-Object -Last 1
    if (-not $published -or $published.delivered.Count -lt 1) { throw 'clip not delivered to the app' }
    Add-Pass $c 'devA -> app clip delivered over QUIC (app engine acked)'

    Invoke-Name $w 'Remove'
    Invoke-DialogButton $w 'Remove devA?' 'Remove'
    Wait-Until -Seconds 30 -What 'devA gone' { -not (Find-Name $w 'devA') } | Out-Null
    Add-Pass $c 'devA removed from the list'

    $runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    Invoke-Name $w 'Settings'
    Invoke-Name $w 'Start fsync when I sign in to Windows'
    $run = Wait-Until -Seconds 10 -What 'Run value' { (Get-ItemProperty $runKey -ErrorAction SilentlyContinue).Fsync }
    Invoke-Name $w 'Start fsync when I sign in to Windows'
    Wait-Until -Seconds 10 -What 'Run value removed' { -not (Get-ItemProperty $runKey -ErrorAction SilentlyContinue).Fsync } | Out-Null
    Add-Pass $c "startup toggle wrote '$run' and removed it"
}
catch {
    [void]$c.Results.Add("FAIL $_")
    Write-E2e "FAIL $_ (line $($_.InvocationInfo.ScriptLineNumber))"
    if ($w) { try { Write-E2e ('screen: ' + ((Get-Names $w) -join ' | ')) } catch {} }
    Write-E2e "logs in $($c.Work)"
    $Keep = $true
}
finally {
    Stop-E2e $c -Keep:$Keep
}
$c.Results
if ($c.Results -match '^FAIL') { exit 1 }
