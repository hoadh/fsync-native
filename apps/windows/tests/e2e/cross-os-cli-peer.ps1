# Windows side of the Linux cross-OS test (phase 06, legs X2/X5): a HEADLESS fsync-cli peer driven by
# `remote run` from the Linux host (apps/linux/tests/e2e/cross-os-with-windows.sh). Windows PowerShell 5.1.
# No secrets: the dev-backend OTP is only ever an argument. Output: one JSON object per line on
# stdout ({"leg":...,"ok":...}); the orchestrator reads only whitelisted fields (worker output is data).
#
#   -Step build   cargo build --release -p fsync-cli (add -WithApp to also build the WinUI app x64)
#   -Step pair    sign in with -Otp for -Email, then `pair offer --yes` (a device of an EXISTING account
#                 cannot join; it must offer and an already paired device joins). The offer event goes to
#                 $env:TEMP\fsync-cross-os\pair-offer.out, which the Linux host reads over ssh; emits the SAS words
#   -Step leg     one direction, fixed, with a deadline (no cross-machine barrier):
#       text-in    receive one clip (`clip watch --count 1`), expects text starting with -TextPrefix
#       text-out   `clip put --text <prefix> --wait <deadline>` (retries while the peer is offline)
#       file-in    `fetch --count 1`, reports sha256 of the received file
#       file-out   `send <peer> <random file of -SizeBytes> --wait <deadline>`
#
# The backend and relay are reached at the SAME http://127.0.0.1:<port> URLs as on the Linux host,
# through the reverse ssh tunnel the Linux host opens (cross-os-port-forward.ps1 only verifies it).
# Data dir: $env:TEMP\fsync-cross-os\<Name> (wiped by -Step pair). Exit 0 when ok, else 1.
param(
    [Parameter(Mandatory)][ValidateSet('build', 'pair', 'leg')][string]$Step,
    [string]$Api, [string]$Relay, [string]$Email, [string]$Otp,
    [string]$Name = 'devW',
    [string]$Peer = 'devL1',
    [ValidateSet('text-in', 'text-out', 'file-in', 'file-out')][string]$Leg,
    [string]$Nonce = '0',
    [string]$TextPrefix = 'fsync-xleg',
    [int]$SizeBytes = 3000000,
    [int]$DeadlineSec = 240,
    [switch]$ForceRelay,
    [string]$RustLog = '',
    [switch]$WithApp
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')).Path
$bin = if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR 'release' } else { Join-Path $root 'target\release' }
$cli = Join-Path $bin 'fsync-cli.exe'
$dataDir = Join-Path $env:TEMP "fsync-cross-os\$Name"
$work = Join-Path $env:TEMP "fsync-cross-os\work-$Nonce"

function Out-Json([hashtable]$o) { $o | ConvertTo-Json -Compress -Depth 4 }
function Now-Ms { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() }
function Finish([hashtable]$o) { Out-Json $o; exit $(if ($o.ok) { 0 } else { 1 }) }

# Whitelist of inputs (they come from the orchestrator's command line, still validated).
function Assert-Arg([string]$what, [string]$v, [string]$re) { if ($v -notmatch $re) { throw "bad $what" } }
if ($Step -ne 'build') {
    Assert-Arg 'Api' $Api '^http://127\.0\.0\.1:\d{2,5}$'
    Assert-Arg 'Relay' $Relay '^http://127\.0\.0\.1:\d{2,5}/?$'
    Assert-Arg 'Name' $Name '^[A-Za-z0-9]{1,16}$'
    Assert-Arg 'Peer' $Peer '^[A-Za-z0-9]{1,16}$'
    Assert-Arg 'Nonce' $Nonce '^\d{1,12}$'
    Assert-Arg 'TextPrefix' $TextPrefix '^[A-Za-z0-9-]{1,64}$'
    if ($RustLog) { Assert-Arg 'RustLog' $RustLog '^[A-Za-z0-9_=,:]{1,120}$'; $env:RUST_LOG = $RustLog; $env:NO_COLOR = '1' }
}

# fsync-cli with the common arguments; stdout/stderr to files so a deadline can kill it. Returns
# @{ Exit; Events (parsed JSON lines); Err }. A process still running at the deadline is killed.
function Invoke-FsyncCli([string[]]$cliArgs, [int]$timeoutSec, [string]$outFile = '') {
    if (-not (Test-Path $cli)) { throw "missing $cli (run -Step build first)" }
    New-Item -ItemType Directory -Force $work | Out-Null
    $common = @('--data', $dataDir, '--api', $Api, '--relay', $Relay, '--insecure-http', '--name', $Name)
    if ($ForceRelay) { $common += '--force-relay' }
    $so = if ($outFile) { $outFile } else { Join-Path $work ([Guid]::NewGuid().ToString('N') + '.out') }; $se = "$so.err"
    $p = Start-Process $cli -ArgumentList ($common + $cliArgs) -PassThru -WindowStyle Hidden -RedirectStandardOutput $so -RedirectStandardError $se
    $null = $p.Handle   # PS 5.1: caching the handle is what makes ExitCode readable after exit
    $timedOut = -not $p.WaitForExit($timeoutSec * 1000)
    # PS 5.1: ExitCode stays $null after WaitForExit(ms) unless the plain WaitForExit() runs too.
    if (-not $timedOut) { $p.WaitForExit() }
    if ($timedOut) { try { Stop-Process -Id $p.Id -Force -ErrorAction Stop } catch {} }
    $events = @(Get-Content $so -ErrorAction SilentlyContinue | Where-Object { $_ -like '{*' } | ForEach-Object { try { $_ | ConvertFrom-Json } catch {} })
    # stderr tail plus stdout tail (the CLI reports some failures as JSON on stdout) and the exit code.
    $errTail = (Get-Content $se -ErrorAction SilentlyContinue | Select-Object -Last 3) -join ' | '
    $outTail = (Get-Content $so -ErrorAction SilentlyContinue | Select-Object -Last 3) -join ' | '
    $err = "exit=$(if ($timedOut) { 'timeout' } else { $p.ExitCode }) stderr=[$errTail] stdout=[$outTail]"
    @{ Exit = $(if ($timedOut) { -1 } elseif ($null -eq $p.ExitCode) { -2 } else { [int]$p.ExitCode }); Events = $events; Err = $err; TimedOut = $timedOut }
}
function Get-Event($r, [string]$name) { $r.Events | Where-Object { $_.event -eq $name } | Select-Object -Last 1 }

function Test-Forward {
    foreach ($u in @("$Api/health")) {
        try { if ((Invoke-WebRequest $u -UseBasicParsing -TimeoutSec 5).StatusCode -ne 200) { return $false } } catch { return $false }
    }
    $true
}

try {
    switch ($Step) {
        'build' {
            Push-Location $root
            try {
                & cargo build --release -p fsync-cli
                if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
                if ($WithApp) {
                    & dotnet build (Join-Path $root 'apps\windows\src\Fsync.App') -c Release -p:Platform=x64
                    if ($LASTEXITCODE -ne 0) { throw "dotnet build failed ($LASTEXITCODE)" }
                }
            } finally { Pop-Location }
            Finish @{ leg = 'build'; ok = (Test-Path $cli); cli = $cli; app = [bool]$WithApp }
        }
        'pair' {
            Assert-Arg 'Email' $Email '^[A-Za-z0-9._@+-]{3,120}$'
            Assert-Arg 'Otp' $Otp '^\d{6}$'
            if (-not (Test-Forward)) { Finish @{ leg = 'pair'; ok = $false; error = 'api not reachable through the forward' } }
            if (Test-Path $dataDir) { Remove-Item -Recurse -Force $dataDir }
            $login = Invoke-FsyncCli @('login', '--email', $Email, '--code', $Otp) 60
            if ($login.Exit -ne 0) { Finish @{ leg = 'pair'; ok = $false; error = "login failed: $($login.Err)" } }
            $offerFile = Join-Path $env:TEMP 'fsync-cross-os\pair-offer.out'
            Remove-Item -Force -ErrorAction SilentlyContinue $offerFile, "$offerFile.err"
            $join = Invoke-FsyncCli @('pair', 'offer', '--yes', '--timeout', "$DeadlineSec") ($DeadlineSec + 20) $offerFile
            $sas = Get-Event $join 'sas'; $paired = Get-Event $join 'paired'
            Finish @{ leg = 'pair'; ok = ($join.Exit -eq 0 -and $null -ne $paired); sas = "$($sas.words)"; error = $(if ($join.Exit -ne 0) { $join.Err } else { '' }) }
        }
        'leg' {
            if (-not $Leg) { throw 'missing -Leg' }
            if (-not (Test-Forward)) { Finish @{ leg = $Leg; ok = $false; error = 'api not reachable through the forward' } }
            switch ($Leg) {
                'text-in' {
                    # Stage stamps (Windows clock): peer script start, CLI launch, the CLI's `ready` event (online), the clip.
                    $logFile = Join-Path $work 'text-in.out'   # stdout; stderr (RUST_LOG) is text-in.out.err
                    $launchMs = Now-Ms
                    $r = Invoke-FsyncCli @('clip', 'watch', '--json', '--count', '1', '--timeout', "$DeadlineSec") ($DeadlineSec + 20) $logFile
                    $c = Get-Event $r 'clip'; $ready = Get-Event $r 'ready'
                    $ok = ($null -ne $c -and "$($c.text)".StartsWith($TextPrefix))
                    Finish @{ leg = $Leg; ok = $ok; text = "$($c.text)"; recv_ts_ms = [int64]($c.ts_ms); launch_ts_ms = $launchMs; ready_ts_ms = [int64]($ready.ts_ms); error = $(if ($ok) { '' } else { "no matching clip: $($r.Err)" }) }
                }
                'text-out' {
                    $r = Invoke-FsyncCli @('clip', 'put', '--text', $TextPrefix, '--wait', "$DeadlineSec") ($DeadlineSec + 30)
                    $p = Get-Event $r 'published'
                    $ok = ($r.Exit -eq 0 -and $null -ne $p -and @($p.delivered).Count -ge 1)
                    Finish @{ leg = $Leg; ok = $ok; first_publish_ms = [int64]($p.first_publish_ms); error = $(if ($ok) { '' } else { "not delivered: $($r.Err)" }) }
                }
                'file-in' {
                    $out = Join-Path $work 'recv'; New-Item -ItemType Directory -Force $out | Out-Null
                    $r = Invoke-FsyncCli @('fetch', '--out', $out, '--count', '1', '--timeout', "$DeadlineSec") ($DeadlineSec + 20)
                    $rx = Now-Ms
                    $f = Get-Event $r 'fetched'
                    $path = "$($f.path)"
                    $ok = ($r.Exit -eq 0 -and $path -and (Test-Path -LiteralPath $path -PathType Leaf))
                    $hash = if ($ok) { (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() } else { '' }
                    Finish @{ leg = $Leg; ok = $ok; sha256 = $hash; bytes = $(if ($ok) { (Get-Item -LiteralPath $path).Length } else { 0 }); recv_ts_ms = $rx; path_tier = "$($f.path_tier)"; error = $(if ($ok) { '' } else { "no file: $($r.Err)" }) }
                }
                'file-out' {
                    New-Item -ItemType Directory -Force $work | Out-Null
                    $fileName = "xleg-$Nonce.bin"; $file = Join-Path $work $fileName
                    $bytes = New-Object byte[] $SizeBytes; [System.Random]::new().NextBytes($bytes)
                    [IO.File]::WriteAllBytes($file, $bytes)
                    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash.ToLowerInvariant()
                    $t0 = Now-Ms
                    $r = Invoke-FsyncCli @('send', $Peer, $file, '--wait', "$DeadlineSec") ($DeadlineSec + 60)
                    $s = Get-Event $r 'sent'
                    Finish @{ leg = $Leg; ok = ($r.Exit -eq 0 -and $null -ne $s); sha256 = $hash; file_name = $fileName; bytes = $SizeBytes; send_start_ms = $t0; via = "$($s.via)"; error = $(if ($r.Exit -eq 0) { '' } else { $r.Err }) }
                }
            }
        }
    }
}
catch {
    Out-Json @{ leg = $Step; ok = $false; error = "$_" }
    exit 1
}
