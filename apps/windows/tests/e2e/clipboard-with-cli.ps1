# Windows app end-to-end (phase 14): clipboard, files and history against fsync-cli and a second
# app instance, on a local dev backend. Needs an interactive desktop session (clipboard, UIA,
# hotkeys), docker, release fsync-backend/fsync-cli, the app (x64 Release) and PasteHelper.
#
#   apps/windows/tests/e2e/clipboard-with-cli.ps1 [-App <Fsync.exe>] [-Relay <url>] [-Keep]
#
# W1 (auto mode, hotkeys off) pairs with CLI devA: text and HTML out, a sensitive copy is not
# sent, text in (clipboard written), history copy-back, devA pushes a file (accepted in the UI),
# Explorer "Send to" (second launch with --send) pushes to devA. W2 (manual mode, owns the
# hotkeys) joins: a file copied on W1 arrives at W2 as virtual files (Ctrl+Alt+V) and is pasted
# by the shell's copy engine, bytes streaming from W1 (read_file_range).
param(
    [string]$App = (Join-Path $PSScriptRoot '..\..\src\Fsync.App\bin\x64\Release\net8.0-windows10.0.19041.0\win-x64\Fsync.exe'),
    [string]$PasteHelper = (Join-Path $PSScriptRoot '..\Fsync.Interop.PasteHelper\bin\Release\net8.0-windows10.0.19041.0\Fsync.Interop.PasteHelper.exe'),
    [string]$Relay = '',
    [switch]$Keep
)
$ErrorActionPreference = 'Continue'
Import-Module (Join-Path $PSScriptRoot 'E2eHost.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'ClipTools.psm1') -Force
foreach ($f in @($App, $PasteHelper)) { if (-not (Test-Path $f)) { throw "missing $f" } }
$c = New-E2eContext (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')) $Relay
$tag = [Guid]::NewGuid().ToString('N').Substring(0, 6)
$w1 = $null; $w2 = $null

try {
    Start-E2eBackend $c
    $email = "win-clip-$PID@example.test"
    New-CliAccount $c 'devA' $email
    New-Item -ItemType Directory (Join-Path $c.Work 'W1') | Out-Null
    '{ "HotkeysEnabled": false }' | Set-Content (Join-Path $c.Work 'W1\preferences.json')
    $p1 = Start-FsyncApp $c $App 'W1'
    $w1 = Get-AppWindow $p1.Id 60
    Connect-AppWithCli $c $w1 $email 'devA'
    Add-Pass $c 'W1 signed in and paired with devA'

    Write-E2e 'clipboard out'
    $watch = Start-Cli $c 'devA' 'watch' @('clip', 'watch', '--json', '--timeout', '300')
    Wait-Until -Seconds 60 -What 'devA watch ready' { Get-CliEvents $watch.Out 'ready' } | Out-Null
    Start-Sleep 2
    $text = "win-to-cli-$tag"; $t0 = Get-Date
    Set-ClipRetry @{ Value = $text }
    Wait-Until -Seconds 60 -What 'text at devA' { Get-CliEvents $watch.Out 'clip' | Where-Object { $_.text -eq $text } } | Out-Null
    Add-Pass $c ("text copied on Windows reached devA in {0:N1} s" -f ((Get-Date) - $t0).TotalSeconds)
    Set-ClipRetry @{ Value = "<b>bold-$tag</b>"; AsHtml = $true }
    Wait-Until -Seconds 60 -What 'html at devA' { Get-CliEvents $watch.Out 'clip' | Where-Object { $_.kind -eq 'rich' } } | Out-Null
    Add-Pass $c 'HTML copied on Windows reached devA as a rich clip'
    $before = @(Get-CliEvents $watch.Out 'clip').Count
    Set-SensitiveText "secret-$tag"
    Start-Sleep 6
    if (@(Get-CliEvents $watch.Out 'clip').Count -ne $before) { throw 'a sensitive copy was sent' }
    Add-Pass $c 'copy marked ExcludeClipboardContentFromMonitorProcessing was not sent'
    $watch.P.Kill(); $watch.P.WaitForExit(10000) | Out-Null

    Write-E2e 'clipboard in'
    $inText = "cli-to-win-$tag"; $t0 = Get-Date
    $put = Start-Cli $c 'devA' 'put' @('clip', 'put', '--text', $inText, '--wait', '60')
    Wait-Until -Seconds 60 -What 'clipboard written' { (Get-ClipText) -eq $inText } | Out-Null
    Add-Pass $c ("text from devA is on the Windows clipboard after {0:N1} s" -f ((Get-Date) - $t0).TotalSeconds)
    $put.P.WaitForExit(60000) | Out-Null

    Invoke-Name $w1 'History'
    Wait-Like $w1 "*$inText*" 30 | Out-Null
    Set-ClipRetry @{ Value = 'something else' }
    Invoke-FirstEnabled $w1 'Copy' # the clip from devA (this PC's own copies cannot be copied back)
    Wait-Until -Seconds 20 -What 'copied back' { (Get-ClipText) -eq $inText } | Out-Null
    Add-Pass $c 'history lists the clip and Copy puts it back on the clipboard'

    Write-E2e 'files'
    $inFile = Join-Path $c.Work "from-devA-$tag.bin"; New-RandomFile $inFile (3MB)
    $send = Start-Cli $c 'devA' 'send' @('send', 'W1', $inFile, '--wait', '60')
    Invoke-Name $w1 'Files'
    Invoke-Name $w1 'Save to Downloads' 60
    $got = Join-Path $c.Work "W1-downloads\from-devA-$tag.bin"
    # A retry after ~20 s is expected here: W1 dialed an earlier devA process (see phase-14 notes).
    Wait-Until -Seconds 90 -What 'pushed file saved' { (Test-Path $got) -and (Get-FileHashHex $got) -eq (Get-FileHashHex $inFile) } | Out-Null
    $send.P.WaitForExit(60000) | Out-Null
    Add-Pass $c 'file pushed by devA was accepted in the Files page and saved intact'

    $outFile = Join-Path $c.Work "to-devA-$tag.bin"; New-RandomFile $outFile (2MB)
    $outDir = Join-Path $c.Work 'devA-fetched'; New-Item -ItemType Directory $outDir | Out-Null
    $fetch = Start-Cli $c 'devA' 'fetch' @('fetch', '--out', $outDir, '--count', '1', '--timeout', '120')
    Wait-Until -Seconds 60 -What 'devA fetch ready' { Get-CliEvents $fetch.Out 'ready' } | Out-Null
    $env:FSYNC_DATA_DIR = Join-Path $c.Work 'W1' # the second launch must find W1's instance
    [void]$c.Procs.Add((Start-Process $App -PassThru -ArgumentList @('--send', $outFile)))
    Invoke-Name $w1 'devA' 30 # device button of the Send-to picker
    Wait-Until -Seconds 90 -What 'devA fetched' { Get-CliEvents $fetch.Out 'fetched' } | Out-Null
    $fetched = Join-Path $outDir (Split-Path $outFile -Leaf)
    if ((Get-FileHashHex $fetched) -ne (Get-FileHashHex $outFile)) { throw 'Send to: hash differs' }
    Add-Pass $c 'Explorer Send to (second launch with --send) pushed the file to devA intact'

    Write-E2e 'virtual-file paste W1 -> W2'
    $p2 = Start-FsyncApp $c $App 'W2'
    $w2 = Get-AppWindow $p2.Id 60
    Connect-AppWithCli $c $w2 $email 'devA'
    Invoke-Name $w2 'Settings'
    Invoke-Name $w2 'Send my clipboard only when I ask (manual mode)'
    Invoke-Name $w2 'Save'
    Wait-Name $w2 'Saved.' 20 | Out-Null
    Invoke-Name $w1 'Devices'
    Wait-Like $w1 '*W2*' 60 | Out-Null
    Add-Pass $c 'W2 paired and in manual mode; W1 lists it'
    $lazy = Join-Path $c.Work "lazy-$tag.bin"; New-RandomFile $lazy (5MB)
    Set-ClipRetry @{ Path = $lazy }
    Wait-Until -Seconds 60 -What 'virtual files on the clipboard (Ctrl+Alt+V on W2)' {
        Send-CtrlAlt 'V'; Start-Sleep -Milliseconds 1500
        (Get-ClipFormats) -contains 'FileGroupDescriptorW'
    } | Out-Null
    $pasteDir = Join-Path $c.Work 'pasted'; New-Item -ItemType Directory $pasteDir | Out-Null
    $t0 = Get-Date
    # stdin stays open: the helper keeps pumping (the copy engine runs in it) until we write a line.
    $psi = New-Object System.Diagnostics.ProcessStartInfo $PasteHelper
    $psi.Arguments = "`"$pasteDir`""; $psi.UseShellExecute = $false; $psi.RedirectStandardInput = $true; $psi.CreateNoWindow = $true
    $helper = [System.Diagnostics.Process]::Start($psi)
    [void]$c.Procs.Add($helper)
    $pasted = Join-Path $pasteDir (Split-Path $lazy -Leaf)
    Wait-Until -Seconds 120 -What 'pasted file complete' { (Test-Path $pasted) -and (Get-FileHashHex $pasted) -eq (Get-FileHashHex $lazy) } | Out-Null
    Add-Pass $c ("Explorer paste of a 5 MiB file copied on W1 streamed into W2's virtual entry in {0:N1} s, hash ok" -f ((Get-Date) - $t0).TotalSeconds)
    $helper.StandardInput.WriteLine("done")
}
catch {
    [void]$c.Results.Add("FAIL $_")
    Write-E2e "FAIL $_ (line $($_.InvocationInfo.ScriptLineNumber))"
    if ($w1) { try { Write-E2e ('W1 screen: ' + ((Get-Names $w1) -join ' | ')) } catch {} }
    if ($w2) { try { Write-E2e ('W2 screen: ' + ((Get-Names $w2) -join ' | ')) } catch {} }
    Write-E2e "logs in $($c.Work)"
    $Keep = $true
}
finally {
    Stop-E2e $c -Keep:$Keep
}
$c.Results
if ($c.Results -match '^FAIL') { exit 1 }
