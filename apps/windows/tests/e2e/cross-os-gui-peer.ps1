# Windows side of the Linux cross-OS GUI legs (phase 06, X3 + X4): the WinUI app (not the CLI) on the
# Rig worker, driven over UI Automation, against the Linux app on the host. Windows PowerShell 5.1
# (the worker has no pwsh), run through `remote run` in an INTERACTIVE task (taskMode interactive,
# a logged-on desktop; the app, clipboard, hotkeys and UIA need it). No secrets: the dev-backend
# OTP is an argument. Driven by apps/linux/tests/e2e/cross-os-with-windows.sh (X3=1).
#
#   -Step build-app   uniffi-bindgen-cs (cargo install, once), scripts/build-windows-dll.ps1 -Rids win-x64,
#                     dotnet build of the app (x64 Release) and Fsync.Interop.PasteHelper
#   -Step pair        X4: signs the app in (OTP), shows the pairing code, waits for the Linux app to join,
#                     reads the SAS words from the screen, accepts, checks Devices lists the peer as Cli
#   -Step legs -Run out | in-auto | in-lazy   X3, one direction per run, ordered by SYNC FILES:
#       out       app copies text, html, png, a file (auto mode); the Linux side says `go-<item>` and `got-<item>`
#       in-auto   Linux offers text, html, png; the app (auto mode) must write the Windows clipboard
#       in-lazy   Linux offers a file clip; manual mode, Win+Alt+V, virtual files pasted by the shell
#                 copy engine (PasteHelper), bytes streamed from the Linux app
# Sync files live in %TEMP%\fsync-cross-os\gui\sync. The Linux host reads/writes them over plain ssh
# (the worker's run output only arrives at the end). Output: JSON lines on stdout. Exit 0 when ok.
# Test items: 40x40 png with pixel (x*7+y*3)%256 on every channel (same formula on the Linux side).
param(
    [Parameter(Mandatory)][ValidateSet('build-app', 'pair', 'legs')][string]$Step,
    [ValidateSet('out', 'in-auto', 'in-lazy')][string]$Run,
    [string]$Api, [string]$Relay, [string]$Email, [string]$Otp,
    [string]$Name = 'devWG',
    [string]$Peer = 'devL1',
    [string]$Nonce = '0',
    [int]$DeadlineSec = 120
)
$ErrorActionPreference = 'Stop'

# `remote run` executes over ssh in session 0: no desktop, and the WinUI app dies on start there
# (Microsoft.UI.Xaml fault 0xc000027b). The pair/legs steps therefore re-run THIS script, with the same
# arguments, as a one-shot scheduled task bound to the logged-on user's desktop session (/it), and
# relay its output and exit code. The user must be logged on (RDP window open).
if ($Step -ne 'build-app' -and [System.Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0) {
    foreach ($k in $PSBoundParameters.Keys) { if ("$($PSBoundParameters[$k])" -notmatch '^[A-Za-z0-9._@+:/-]*$') { throw "bad character in -$k" } }
    $tmp = Join-Path $env:TEMP 'fsync-cross-os\gui-launch'
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    New-Item -ItemType Directory -Force $tmp | Out-Null
    $argText = ($PSBoundParameters.Keys | ForEach-Object { "-$_ '$($PSBoundParameters[$_])'" }) -join ' '
    $outFile = Join-Path $tmp 'out.txt'; $codeFile = Join-Path $tmp 'exit.txt'
    $launcher = Join-Path $tmp 'launch.ps1'
    @(
        "& '$PSCommandPath' $argText *> '$outFile'",
        "Set-Content -Path '$codeFile' -Value `$LASTEXITCODE -Encoding ASCII"
    ) | Set-Content -Path $launcher -Encoding ASCII
    $task = 'fsync-xos-gui'
    & schtasks /create /tn $task /tr "powershell -NoProfile -ExecutionPolicy Bypass -File $launcher" /sc once /st 00:00 /it /ru $env:USERNAME /f | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'schtasks /create failed (is the dev user logged on?)' }
    try {
        & schtasks /run /tn $task | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'schtasks /run failed' }
        $limit = [DateTime]::UtcNow.AddSeconds($DeadlineSec + 120)
        while (-not (Test-Path $codeFile) -and [DateTime]::UtcNow -lt $limit) { Start-Sleep -Milliseconds 500 }
        if (-not (Test-Path $codeFile)) { & schtasks /end /tn $task | Out-Null; Write-Output '{"ok":false,"error":"interactive task did not finish in time"}'; exit 1 }
        if (Test-Path $outFile) { Get-Content $outFile | ForEach-Object { Write-Output $_ } }
        exit ([int](Get-Content $codeFile | Select-Object -First 1))
    } finally { & schtasks /delete /tn $task /f 2>&1 | Out-Null }
}

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')).Path
Import-Module (Join-Path $PSScriptRoot 'Uia.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'ClipTools.psm1') -Force
Add-Type -AssemblyName System.Drawing
$bin = Join-Path $root 'apps\windows\src\Fsync.App\bin\x64\Release\net8.0-windows10.0.19041.0\win-x64'
$exe = Join-Path $bin 'Fsync.exe'   # same default as pair-with-cli.ps1
$pasteHelper = Join-Path $root 'apps\windows\tests\Fsync.Interop.PasteHelper\bin\Release\net8.0-windows10.0.19041.0\Fsync.Interop.PasteHelper.exe'
$base = Join-Path $env:TEMP 'fsync-cross-os\gui'
$dataDir = Join-Path $base 'W'
$sync = Join-Path $base 'sync'
$script:app = $null

function Out-Json([hashtable]$o) { $o | ConvertTo-Json -Compress -Depth 4 }
function Now-Ms { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() }
function Finish([hashtable]$o) { Out-Json $o; exit $(if ($o.ok) { 0 } else { 1 }) }
function Assert-Arg([string]$what, [string]$v, [string]$re) { if ($v -notmatch $re) { throw "bad $what" } }

# --- sync files (the Linux host uses the same names over ssh) ---
function Sync-Put([string]$n, [string]$v = '1') { Set-Content -Path (Join-Path $sync $n) -Value $v -Encoding ASCII }
function Sync-Wait([string]$n, [int]$sec) {
    $p = Join-Path $sync $n
    Wait-Until -Seconds $sec -What "sync file $n" { if (Test-Path $p) { (Get-Content $p -ErrorAction SilentlyContinue | Select-Object -First 1) } } | Out-Null
    (Get-Content $p | Select-Object -First 1)
}
function Sync-Has([string]$n) { Test-Path (Join-Path $sync $n) }

# --- app lifecycle ---
function Stop-GuiApps {
    Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe -or $_.ProcessName -eq 'Fsync.Interop.PasteHelper' } |
        ForEach-Object { try { Stop-Process -Id $_.Id -Force -ErrorAction Stop } catch {} }
}
function Set-Prefs([bool]$hotkeys) {
    New-Item -ItemType Directory -Force $dataDir | Out-Null
    ('{ "HotkeysEnabled": ' + $hotkeys.ToString().ToLowerInvariant() + ' }') | Set-Content (Join-Path $dataDir 'preferences.json')
}
function Start-GuiApp {
    if (-not (Test-Path $exe)) { throw "missing app $exe (run -Step build-app)" }
    $env:FSYNC_DATA_DIR = $dataDir; $env:FSYNC_API = $Api; $env:FSYNC_RELAY = $Relay
    $env:FSYNC_DEVICE_NAME = $Name; $env:FSYNC_DOWNLOAD_DIR = Join-Path $base 'downloads'
    $p = Start-Process $exe -PassThru
    $null = $p.Handle   # PS 5.1: caching the handle keeps ExitCode readable
    $script:app = $p
    Get-AppWindow $p.Id 90
}
function Test-Forward { try { (Invoke-WebRequest "$Api/health" -UseBasicParsing -TimeoutSec 5).StatusCode -eq 200 } catch { $false } }

# Toggle by UIA name to the wanted state, then Save.
function Set-Toggle($w, [string]$toggleName, [bool]$on) {
    Invoke-Name $w 'Settings'
    $el = Wait-Name $w $toggleName 30
    $tp = $el.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
    $isOn = ($tp.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::On)
    if ($isOn -ne $on) { $tp.Toggle(); Invoke-Name $w 'Save'; Wait-Name $w 'Saved.' 20 | Out-Null }
}
$manualName = 'Send my clipboard only when I ask (manual mode)'

function Wait-PeerOnline($w) {
    Invoke-Name $w 'Devices'
    Wait-Like $w "*$Peer*Online*" 90 | Out-Null
}

# --- png helpers: pixel (x*7+y*3)%256 on every channel ---
function New-TestBitmap([int]$n) {
    $bmp = New-Object System.Drawing.Bitmap $n, $n
    for ($y = 0; $y -lt $n; $y++) { for ($x = 0; $x -lt $n; $x++) { $v = ($x * 7 + $y * 3) % 256; $bmp.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, $v, $v, $v)) } }
    $bmp
}
function Test-BitmapPixels($img, [int]$n) {
    if ($img.Width -ne $n -or $img.Height -ne $n) { return $false }
    $bmp = New-Object System.Drawing.Bitmap $img
    for ($y = 0; $y -lt $n; $y++) { for ($x = 0; $x -lt $n; $x++) {
        $v = ($x * 7 + $y * 3) % 256; $c = $bmp.GetPixel($x, $y)
        if ($c.R -ne $v -or $c.G -ne $v -or $c.B -ne $v) { return $false } } }
    $true
}
function Set-ClipImage($bmp) {
    for ($i = 0; ; $i++) { try { [System.Windows.Forms.Clipboard]::SetImage($bmp); return } catch { if ($i -ge 20) { throw }; Start-Sleep -Milliseconds 100 } }
}

try {
    switch ($Step) {
        'build-app' {
            Push-Location $root
            try {
                if (-not (Get-Command uniffi-bindgen-cs -ErrorAction SilentlyContinue)) {
                    & cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag 'v0.10.0+v0.29.4'
                    if ($LASTEXITCODE -ne 0) { throw "cargo install uniffi-bindgen-cs failed ($LASTEXITCODE)" }
                }
                & (Join-Path $root 'scripts\build-windows-dll.ps1') -Rids win-x64
                if (-not $?) { throw 'build-windows-dll.ps1 failed' }
                & dotnet build (Join-Path $root 'apps\windows\src\Fsync.App') -c Release -p:Platform=x64 -r win-x64
                if ($LASTEXITCODE -ne 0) { throw "dotnet build app failed ($LASTEXITCODE)" }
                & dotnet build (Join-Path $root 'apps\windows\tests\Fsync.Interop.PasteHelper') -c Release
                if ($LASTEXITCODE -ne 0) { throw "dotnet build PasteHelper failed ($LASTEXITCODE)" }
            } finally { Pop-Location }
            Finish @{ leg = 'build-app'; ok = ((Test-Path $exe) -and (Test-Path $pasteHelper)); app = $exe }
        }
        'pair' {
            Assert-Arg 'Api' $Api '^http://127\.0\.0\.1:\d{2,5}$'
            Assert-Arg 'Relay' $Relay '^http://127\.0\.0\.1:\d{2,5}/?$'
            Assert-Arg 'Email' $Email '^[A-Za-z0-9._@+-]{3,120}$'
            Assert-Arg 'Otp' $Otp '^\d{6}$'
            Assert-Arg 'Name' $Name '^[A-Za-z0-9]{1,16}$'
            Assert-Arg 'Peer' $Peer '^[A-Za-z0-9]{1,16}$'
            if (-not (Test-Forward)) { Finish @{ leg = 'pair'; ok = $false; error = 'api not reachable through the tunnel' } }
            Stop-GuiApps; Start-Sleep 1
            if (Test-Path $base) { Remove-Item -Recurse -Force $base }
            New-Item -ItemType Directory -Force $sync | Out-Null
            Set-Prefs $false
            $w = Start-GuiApp
            Set-Text $w 'Email' $Email
            Invoke-Name $w 'Send code'
            Start-Sleep -Milliseconds 500
            Set-Text $w 'Code' $Otp
            Invoke-Name $w 'Sign in'
            Invoke-Name $w 'Show a code' 30
            $code = Wait-Until -Seconds 30 -What 'pairing code on screen' { Get-Names $w | Where-Object { $_ -match '^\d{6}$' } | Select-Object -First 1 }
            Sync-Put 'pair-code' $code
            Wait-Name $w 'The words match' $DeadlineSec | Out-Null
            $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'SasText')
            $sas = $w.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c).Current.Name
            Sync-Put 'pair-sas' $sas
            Sync-Wait 'pair-sas-confirmed' 60 | Out-Null   # the host compared the words with the Linux app's
            Invoke-Name $w 'The words match'
            Wait-Name $w 'Paired' 60 | Out-Null
            Invoke-Name $w 'Devices'
            $row = Wait-Like $w "*$Peer*" 60
            $rowText = $row.Current.Name
            # The Devices row reads "<name>, <platform label>, <presence>, <link>"; Platform.Cli is labelled "Command line".
            $isCli = $rowText -like "*$Peer*Command line*"
            Finish @{ leg = 'pair'; ok = $true; sas = "$sas"; peer_row = "$rowText"; peer_listed_as_cli = $isCli }
        }
        'legs' {
            if (-not $Run) { throw 'missing -Run' }
            Assert-Arg 'Api' $Api '^http://127\.0\.0\.1:\d{2,5}$'
            Assert-Arg 'Relay' $Relay '^http://127\.0\.0\.1:\d{2,5}/?$'
            Assert-Arg 'Nonce' $Nonce '^\d{1,12}$'
            if (-not (Test-Forward)) { Finish @{ leg = $Run; ok = $false; error = 'api not reachable through the tunnel' } }
            Stop-GuiApps; Start-Sleep 1
            if (Test-Path $sync) { Remove-Item -Recurse -Force $sync }
            New-Item -ItemType Directory -Force $sync | Out-Null
            Set-Prefs ($Run -eq 'in-lazy')   # lazy paste needs the hotkey (Win+Alt+V); auto runs keep it off
            $w = Start-GuiApp
            Set-Toggle $w $manualName ($Run -eq 'in-lazy')
            Wait-PeerOnline $w
            $res = @{ leg = $Run; ok = $true; items = @{} }
            switch ($Run) {
                'out' {
                    $items = @('text', 'html', 'png', 'file')
                    $fileSha = ''
                    Sync-Put 'ready'
                    foreach ($it in $items) {
                        Sync-Wait "go-$it" $DeadlineSec | Out-Null
                        $file = Join-Path $base "gui-$Nonce-out.bin"
                        if ($it -eq 'file' -and -not (Test-Path $file)) { New-RandomFile $file 3MB; $fileSha = (Get-FileHashHex $file).ToLowerInvariant(); Sync-Put 'file-sha' $fileSha }
                        for ($try = 1; $try -le 4; $try++) {
                            Sync-Put "sent-$it" (Now-Ms)
                            if ($it -eq 'text') { Set-ClipRetry @{ Value = "fsync-gui-$Nonce-text" } }
                            if ($it -eq 'html') { Set-ClipRetry @{ Value = "<b>fsync-gui-$Nonce-html</b>"; AsHtml = $true } }
                            if ($it -eq 'png') { Set-ClipImage (New-TestBitmap 40) }
                            if ($it -eq 'file') { Set-ClipRetry @{ Path = $file } }
                            # rdpclip can re-post the clipboard with the no-history flag right after our write: copy again.
                            try { Sync-Wait "got-$it" 15 | Out-Null; break } catch { if ($try -eq 4) { $res.ok = $false; $res.items[$it] = 'not received' } }
                        }
                    }
                    $res.file_sha256 = $fileSha
                }
                'in-auto' {
                    foreach ($it in @('text', 'html', 'png')) {
                        # No barrier on the offer itself: the host offers after `ready-<item>`, this side polls the
                        # clipboard at once and stamps the first sight (the sync-file round trip is not in the latency).
                        Sync-Put "ready-$it"
                        $okItem = $false
                        try {
                            Wait-Until -Seconds $DeadlineSec -What "$it on the Windows clipboard" {
                                try {
                                    if ($it -eq 'text') { (Get-ClipText) -eq "fsync-gui-$Nonce-in-text" }
                                    elseif ($it -eq 'html') { "$([System.Windows.Forms.Clipboard]::GetData('HTML Format'))" -like "*fsync-gui-$Nonce-in-html*" }
                                    else { [System.Windows.Forms.Clipboard]::ContainsImage() -and (Test-BitmapPixels ([System.Windows.Forms.Clipboard]::GetImage()) 40) }
                                } catch { $false }
                            } | Out-Null
                            $okItem = $true
                        } catch { $okItem = $false }
                        $seenMs = Now-Ms
                        Sync-Put "recv-$it" $seenMs
                        if (-not $okItem) { $res.ok = $false }
                        $res.items[$it] = $okItem
                    }
                }
                'in-lazy' {
                    if (-not (Test-Path $pasteHelper)) { throw "missing $pasteHelper (run -Step build-app)" }
                    Sync-Put 'ready-file'
                    Wait-Until -Seconds $DeadlineSec -What 'virtual files on the clipboard (Win+Alt+V)' {
                        Send-WinAlt 'V'; Start-Sleep -Milliseconds 1500
                        (Get-ClipFormats) -contains 'FileGroupDescriptorW'
                    } | Out-Null
                    $pasteDir = Join-Path $base 'pasted'; New-Item -ItemType Directory -Force $pasteDir | Out-Null
                    $psi = New-Object System.Diagnostics.ProcessStartInfo $pasteHelper
                    $psi.Arguments = "`"$pasteDir`""; $psi.UseShellExecute = $false; $psi.RedirectStandardInput = $true; $psi.CreateNoWindow = $true
                    $helper = [System.Diagnostics.Process]::Start($psi)
                    $pasted = Join-Path $pasteDir "gui-$Nonce-in.bin"
                    Wait-Until -Seconds 120 -What 'pasted file closed' { Test-FileClosed $pasted } | Out-Null
                    Sync-Put 'recv-file' (Now-Ms)
                    $res.file_sha256 = (Get-FileHashHex $pasted).ToLowerInvariant()
                    $res.bytes = (Get-Item $pasted).Length
                    try { $helper.StandardInput.WriteLine('done') } catch {}
                    Set-Toggle $w $manualName $false   # leave the app in auto mode
                }
            }
            Finish $res
        }
    }
}
catch {
    Out-Json @{ leg = $Step; ok = $false; error = "$_"; line = $_.InvocationInfo.ScriptLineNumber; screen = $(if ($w) { try { ((Get-Names $w) | Select-Object -First 40) -join ' | ' } catch { '' } } else { '' }) }
    exit 1
}
finally {
    Stop-GuiApps
}
