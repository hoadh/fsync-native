# Minimal UI Automation helpers for driving the WinUI 3 app in end-to-end tests
# (Windows PowerShell 5.1: UIAutomationClient ships with .NET Framework).
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$script:AE = [System.Windows.Automation.AutomationElement]
$script:Tree = [System.Windows.Automation.TreeScope]

function Wait-Until([scriptblock]$Condition, [int]$Seconds = 30, [string]$What = 'condition') {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $r = & $Condition
        if ($r) { return $r }
        Start-Sleep -Milliseconds 250
    }
    throw "timeout after ${Seconds}s waiting for $What"
}

# Top-level window of a process (WinUI windows can take a moment to appear).
function Get-AppWindow([int]$ProcessId, [int]$Seconds = 30) {
    Wait-Until -Seconds $Seconds -What "window of pid $ProcessId" {
        $c = New-Object System.Windows.Automation.PropertyCondition($script:AE::ProcessIdProperty, $ProcessId)
        $script:AE::RootElement.FindAll($script:Tree::Children, $c) |
            Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Window } | Select-Object -First 1
    }
}

function Find-Name($Root, [string]$Name) {
    $c = New-Object System.Windows.Automation.PropertyCondition($script:AE::NameProperty, $Name)
    $Root.FindFirst($script:Tree::Descendants, $c)
}

function Find-Like($Root, [string]$Pattern) {
    $Root.FindAll($script:Tree::Descendants, [System.Windows.Automation.Condition]::TrueCondition) |
        Where-Object { $_.Current.Name -like $Pattern } | Select-Object -First 1
}

function Wait-Name($Root, [string]$Name, [int]$Seconds = 30) {
    Wait-Until -Seconds $Seconds -What "'$Name'" { Find-Name $Root $Name }
}

function Wait-Like($Root, [string]$Pattern, [int]$Seconds = 30) {
    Wait-Until -Seconds $Seconds -What "'$Pattern'" { Find-Like $Root $Pattern }
}

# Text boxes: the WinUI Header becomes the UIA name.
function Set-Text($Root, [string]$Name, [string]$Text) {
    $el = Wait-Name $Root $Name
    $el.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Text)
}

function Invoke-Name($Root, [string]$Name, [int]$Seconds = 30) {
    $el = Wait-Until -Seconds $Seconds -What "enabled '$Name'" {
        $e = Find-Name $Root $Name
        if ($e -and $e.Current.IsEnabled) { $e }
    }
    $p = $null
    if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke(); return }
    if ($el.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$p)) { $p.Toggle(); return }
    if ($el.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$p)) { $p.Select(); return }
    throw "'$Name' cannot be invoked"
}

# All non-empty element names (debug dumps, reading list rows).
function Get-Names($Root) {
    $Root.FindAll($script:Tree::Descendants, [System.Windows.Automation.Condition]::TrueCondition) |
        ForEach-Object { $_.Current.Name } | Where-Object { $_ }
}

Export-ModuleMember -Function *
