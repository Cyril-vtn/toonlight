# Plays the part of a screen reader while the app runs a script: it reads the app's whole
# accessibility tree again and again, and keeps asking the app to give the keyboard focus to its
# small buttons. Used by CI on Windows. Fails when the app stops with an error. (While the app is
# built without its accessibility tree, there is next to nothing to read: the walk then only
# shows that being looked at does no harm.)
param(
    [Parameter(Mandatory = $true)][string]$App,
    [Parameter(Mandatory = $true)][string]$Script
)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$errors = Join-Path $env:RUNNER_TEMP "reader-errors.log"
$output = Join-Path $env:RUNNER_TEMP "reader-output.log"
$p = Start-Process -FilePath $App -ArgumentList "--script", $Script -PassThru `
    -RedirectStandardError $errors -RedirectStandardOutput $output
$null = $p.Handle   # so that the exit code can be read later
$ofProcess = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$everything = [System.Windows.Automation.Condition]::TrueCondition
$seen = 0
$asked = 0
while (-not $p.HasExited) {
    try {
        $window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst(
            [System.Windows.Automation.TreeScope]::Children, $ofProcess)
        if ($window) {
            $all = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $everything)
            $seen = [Math]::Max($seen, $all.Count)
            foreach ($element in $all) {
                # the panels' small buttons: the ones that vanish when a panel moves
                if ($element.Current.Name.Length -le 2) {
                    try { $element.SetFocus(); $asked++ } catch { }
                }
            }
        }
    } catch { }
}
$p.WaitForExit()
Get-Content $errors -ErrorAction SilentlyContinue
Write-Host "exit code: $($p.ExitCode); elements read: $seen; focus requests: $asked"
if ($p.ExitCode -ne 0) { Write-Host "the app stopped with an error"; exit 1 }
