# Clipboard and keyboard helpers for the clipboard e2e (Windows PowerShell 5.1, STA).
Add-Type -AssemblyName System.Windows.Forms
Add-Type -Namespace FsyncE2e -Name Keys -MemberDefinition @'
[DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, System.UIntPtr extra);
public static void Chord(byte[] mods, byte key) {
    foreach (var m in mods) keybd_event(m, 0, 0, System.UIntPtr.Zero);
    keybd_event(key, 0, 0, System.UIntPtr.Zero);
    keybd_event(key, 0, 2, System.UIntPtr.Zero);
    for (int i = mods.Length - 1; i >= 0; i--) keybd_event(mods[i], 0, 2, System.UIntPtr.Zero);
}
'@

# Win+Alt+<letter> (global hotkeys of the app; Ctrl+Alt would be AltGr).
function Send-WinAlt([char]$letter) { [FsyncE2e.Keys]::Chord([byte[]](0x5B, 0x12), [byte][char]::ToUpper($letter)) }

# Text plus ExcludeClipboardContentFromMonitorProcessing, as a password manager writes it.
function Set-SensitiveText([string]$text) {
    $d = New-Object System.Windows.Forms.DataObject
    $d.SetData([System.Windows.Forms.DataFormats]::UnicodeText, $text)
    $d.SetData('ExcludeClipboardContentFromMonitorProcessing', (New-Object System.IO.MemoryStream(, [byte[]](1, 0, 0, 0))))
    [System.Windows.Forms.Clipboard]::SetDataObject($d, $true)
}

# Format names on the clipboard (no content).
function Get-ClipFormats {
    try { [System.Windows.Forms.Clipboard]::GetDataObject().GetFormats() } catch { @() }
}

function Get-FileHashHex([string]$path) { (Get-FileHash -Algorithm SHA256 $path).Hash }

# True once nobody holds $path open (an exclusive open succeeds): the writer has finished.
function Test-FileClosed([string]$path) {
    if (-not (Test-Path $path)) { return $false }
    try { [System.IO.File]::Open($path, 'Open', 'Read', 'None').Dispose(); $true } catch { $false }
}

function New-RandomFile([string]$path, [int]$bytes) {
    $buf = New-Object byte[] $bytes
    (New-Object System.Random).NextBytes($buf)
    [System.IO.File]::WriteAllBytes($path, $buf)
}

# Clipboard text, or $null while another process holds the clipboard (Get-Clipboard throws then).
function Get-ClipText { try { Get-Clipboard -ErrorAction Stop } catch { $null } }

# Set-Clipboard with retries (the clipboard is briefly locked while apps read a change).
function Set-ClipRetry([hashtable]$a) {
    for ($i = 0; ; $i++) {
        try { Set-Clipboard @a -ErrorAction Stop; return } catch { if ($i -ge 20) { throw }; Start-Sleep -Milliseconds 100 }
    }
}

Export-ModuleMember -Function *
