# Shared setup for Windows app end-to-end scripts: Postgres (docker) + dev backend, fsync-cli
# devices, app instances signed in and paired over UI Automation. State lives in a context
# hashtable ($c) created by New-E2eContext; Stop-E2e cleans up.
Import-Module (Join-Path $PSScriptRoot 'Uia.psm1')

function New-E2eContext([string]$Root, [string]$Relay = '') {
    $bin = if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR 'release' } else { Join-Path $Root 'target\release' }
    $c = @{
        Root = $Root; Cli = Join-Path $bin 'fsync-cli.exe'; Backend = Join-Path $bin 'fsync-backend.exe'; Relay = $Relay
        Work = Join-Path $env:TEMP ('fsync-win-e2e-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
        Pg = "fsync-win-e2e-pg-$PID"; Procs = New-Object System.Collections.ArrayList; Results = New-Object System.Collections.ArrayList
    }
    foreach ($b in @($c.Cli, $c.Backend)) { if (-not (Test-Path $b)) { throw "missing $b (cargo build --release -p fsync-backend -p fsync-cli)" } }
    New-Item -ItemType Directory $c.Work -ErrorAction Stop | Out-Null
    $c
}

function Write-E2e($m) { Write-Host "[win-e2e] $m" }
function Add-Pass($c, $m) { [void]$c.Results.Add("PASS $m"); Write-E2e "PASS $m" }

function Get-FreePort {
    $l = [System.Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0); $l.Start(); $p = $l.LocalEndpoint.Port; $l.Stop(); $p
}

# Postgres 17 in docker (default).
function Start-DockerPostgres($c) {
    docker run -d --name $c.Pg -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=fsync -p 127.0.0.1::5432 postgres:17 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'docker run failed (is Docker Desktop running? or set FSYNC_E2E_PG_BIN)' }
    $c.Docker = $true
    $port = ((docker port $c.Pg 5432/tcp) | Select-Object -First 1) -replace '.*:', ''
    Wait-Until -Seconds 60 -What 'postgres' { docker exec $c.Pg pg_isready -U postgres -d fsync 2>$null | Out-Null; $LASTEXITCODE -eq 0 } | Out-Null
    Start-Sleep 1
    $port
}

# Postgres from local binaries (FSYNC_E2E_PG_BIN = folder with initdb.exe, pg_ctl.exe): a
# throwaway cluster in the work dir, trust auth on 127.0.0.1, for machines without docker.
function Start-LocalPostgres($c) {
    $bin = $env:FSYNC_E2E_PG_BIN
    $c.PgData = Join-Path $c.Work 'pgdata'
    $port = Get-FreePort
    & (Join-Path $bin 'initdb.exe') -D $c.PgData -U postgres --auth=trust -E UTF8 *> (Join-Path $c.Work 'initdb.log')
    if ($LASTEXITCODE -ne 0) { throw 'initdb failed' }
    # Not through a pipe: the server inherits pg_ctl's stdout and would keep the pipe open.
    $psi = New-Object System.Diagnostics.ProcessStartInfo (Join-Path $bin 'pg_ctl.exe')
    $psi.Arguments = "-D `"$($c.PgData)`" -l `"$(Join-Path $c.Work 'postgres.log')`" -o `"-p $port -c listen_addresses=127.0.0.1`" -w start"
    $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
    $pgctl = [System.Diagnostics.Process]::Start($psi)
    if (-not $pgctl.WaitForExit(60000) -or $pgctl.ExitCode -ne 0) { throw 'pg_ctl start failed' }
    & (Join-Path $bin 'createdb.exe') -h 127.0.0.1 -p $port -U postgres fsync
    if ($LASTEXITCODE -ne 0) { throw 'createdb failed' }
    $port
}

# Local iroh relay (spikes/iroh-discovery, as in tests/e2e/p0-exit-demo.sh): peers find each other
# through it; without a relay a receiver cannot dial back to pull files.
function Start-E2eRelay($c) {
    $spike = Join-Path $c.Root 'target\spike\release\iroh-discovery-spike.exe'
    if (-not (Test-Path $spike)) {
        throw "missing $spike (cargo build --release --manifest-path spikes/iroh-discovery/Cargo.toml --target-dir target/spike) or pass -Relay"
    }
    $log = Join-Path $c.Work 'relay.log'
    [void]$c.Procs.Add((Start-Process $spike -PassThru -WindowStyle Hidden -ArgumentList @('relay', (Get-FreePort)) -RedirectStandardOutput $log -RedirectStandardError (Join-Path $c.Work 'relay.err')))
    $c.Relay = Wait-Until -Seconds 20 -What 'relay' { @(Get-Content $log -ErrorAction SilentlyContinue) -match '^RELAY http' | ForEach-Object { $_ -replace '^RELAY ', '' } | Select-Object -First 1 }
}

function Start-E2eBackend($c) {
    if (-not $c.Relay) { Start-E2eRelay $c }
    $pgPort = if ($env:FSYNC_E2E_PG_BIN) { Start-LocalPostgres $c } else { Start-DockerPostgres $c }
    $port = Get-FreePort
    $c.Api = "http://127.0.0.1:$port"
    $env:DATABASE_URL = "postgres://postgres:e2e@127.0.0.1:$pgPort/fsync"
    $env:BIND_ADDR = "127.0.0.1:$port"; $env:FSYNC_DEV_MODE = '1'; $env:NO_COLOR = '1'; $env:RUST_LOG = 'fsync_backend=debug,info'
    $c.BackendLog = Join-Path $c.Work 'backend.log'
    [void]$c.Procs.Add((Start-Process $c.Backend -PassThru -WindowStyle Hidden -RedirectStandardOutput $c.BackendLog -RedirectStandardError (Join-Path $c.Work 'backend.err')))
    Wait-Until -Seconds 30 -What 'backend health' { try { (Invoke-WebRequest "$($c.Api)/health" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200 } catch { $false } } | Out-Null
}

# Newest dev OTP the backend logged for $email.
function Get-Otp($c, [string]$email) {
    Wait-Until -Seconds 15 -What "OTP for $email" {
        $lines = @(Get-Content $c.BackendLog, (Join-Path $c.Work 'backend.err') -ErrorAction SilentlyContinue) -match 'dev otp' -match [regex]::Escape("to=""$email""")
        if ($lines) { ([regex]::Match($lines[-1], 'code="(\d+)"')).Groups[1].Value }
    }
}

# Common fsync-cli arguments for device $name (data dir under the work dir).
function Get-CliArgs($c, [string]$name) {
    $a = @('--data', (Join-Path $c.Work $name), '--api', $c.Api, '--insecure-http', '--name', $name)
    if ($c.Relay) { $a += @('--relay', $c.Relay) }
    $a
}

function Invoke-Cli($c, [string]$name, [string[]]$a) { & $c.Cli @(Get-CliArgs $c $name) @a }

# Background fsync-cli; returns @{ P = process; Out = stdout file }. No spaces in args (PS 5.1).
function Start-Cli($c, [string]$name, [string]$tag, [string[]]$a) {
    $out = Join-Path $c.Work "$name-$tag.out"
    $p = Start-Process $c.Cli -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError (Join-Path $c.Work "$name-$tag.err") `
        -ArgumentList (@(Get-CliArgs $c $name) + $a)
    [void]$c.Procs.Add($p)
    @{ P = $p; Out = $out }
}

function Get-CliEvents([string]$out, [string]$event) {
    @(Get-Content $out -ErrorAction SilentlyContinue) -match """event"":""$event""" | ForEach-Object { $_ | ConvertFrom-Json }
}

# New CLI device that creates the account for $email.
function New-CliAccount($c, [string]$name, [string]$email) {
    Invoke-Cli $c $name @('login', '--email', $email, '--send-only') | Out-Null
    $login = Invoke-Cli $c $name @('login', '--email', $email, '--code', (Get-Otp $c $email))
    if ($login -notmatch '"account":"created"') { throw "$name login: $login" }
}

# Starts the app for data dir $name with extra environment; returns its process.
function Start-FsyncApp($c, [string]$exe, [string]$name, [hashtable]$extraEnv = @{}) {
    $env:FSYNC_DATA_DIR = Join-Path $c.Work $name; $env:FSYNC_API = $c.Api; $env:FSYNC_DEVICE_NAME = $name; $env:FSYNC_RELAY = $c.Relay
    $env:FSYNC_DOWNLOAD_DIR = Join-Path $c.Work "$name-downloads"
    foreach ($k in $extraEnv.Keys) { Set-Item "env:$k" $extraEnv[$k] }
    $p = Start-Process $exe -PassThru
    [void]$c.Procs.Add($p)
    $p
}

# App window $w signs in as $email, shows a pairing code, CLI device $cliName joins; both confirm.
function Connect-AppWithCli($c, $w, [string]$email, [string]$cliName) {
    Set-Text $w 'Email' $email
    Invoke-Name $w 'Send code'
    Start-Sleep -Milliseconds 500
    Set-Text $w 'Code' (Get-Otp $c $email)
    Invoke-Name $w 'Sign in'
    Invoke-Name $w 'Show a code' 30
    $code = Wait-Until -Seconds 30 -What 'pairing code' { Get-Names $w | Where-Object { $_ -match '^\d{6}$' } | Select-Object -First 1 }
    $join = Start-Cli $c $cliName "join-$code" @('pair', 'join', $code, '--yes')
    Wait-Name $w 'The words match' 60 | Out-Null
    Invoke-Name $w 'The words match'
    Wait-Name $w 'Paired' 60 | Out-Null
    if (-not $join.P.WaitForExit(30000) -or -not (Get-CliEvents $join.Out 'paired')) { throw "$cliName did not report paired" }
}

# Invokes the first enabled element named $name (list rows repeat button names).
function Invoke-FirstEnabled($root, [string]$name, [int]$Seconds = 30) {
    $el = Wait-Until -Seconds $Seconds -What "enabled '$name'" {
        $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
        $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond) | Where-Object { $_.Current.IsEnabled } | Select-Object -First 1
    }
    $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}

# Invokes $button of the ContentDialog titled $title once it is open. The dialog's popup comes
# first in the UIA tree, so the first element named $button is the dialog's, not the page's.
function Invoke-DialogButton($root, [string]$title, [string]$button, [int]$Seconds = 15) {
    Wait-Name $root $title $Seconds | Out-Null
    Start-Sleep -Milliseconds 300 # opening animation
    Invoke-Name $root $button $Seconds
}

function Stop-E2e($c, [switch]$Keep) {
    foreach ($p in $c.Procs) { if ($p -and -not $p.HasExited) { try { $p.Kill() } catch {} } }
    if ($c.Docker) { docker rm -f -v $c.Pg 2>$null | Out-Null }
    if ($c.PgData) { & (Join-Path $env:FSYNC_E2E_PG_BIN 'pg_ctl.exe') -D $c.PgData -m fast -w stop *> $null }
    $c.Results | Set-Content (Join-Path $c.Work 'results.txt')
    if (-not $Keep) { Start-Sleep 1; Remove-Item -Recurse -Force $c.Work -ErrorAction SilentlyContinue }
}

Export-ModuleMember -Function *
