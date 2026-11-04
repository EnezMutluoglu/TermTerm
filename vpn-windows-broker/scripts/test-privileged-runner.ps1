$ErrorActionPreference = 'Stop'
$path = Join-Path $PSScriptRoot 'validate-privileged-v3.ps1'
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($path,[ref]$null,[ref]$errors)
if ($errors) { throw ($errors | Out-String) }
$nodes = @($ast.FindAll({param($node) $true},$true))
function Require($value,[string]$message) { if (-not $value) { throw $message } }
$calls = @($nodes | Where-Object { $_ -is [System.Management.Automation.Language.CommandAst] -and $_.CommandElements[0].Extent.Text -eq '$helperPath' })
Require ($calls.Count -gt 0) 'No helper calls found'
$firstCall = ($calls.Extent.StartOffset | Measure-Object -Minimum).Minimum
foreach ($guard in @('Helper hash changed after approval','Wintun hash mismatch')) {
    $check = @($nodes | Where-Object { $_ -is [System.Management.Automation.Language.IfStatementAst] -and $_.Extent.Text.Contains("throw '$guard'") })
    Require ($check.Count -eq 1 -and $check[0].Extent.EndOffset -lt $firstCall) "Helper can execute before pin check: $guard"
}
$init = @($nodes | Where-Object { $_ -is [System.Management.Automation.Language.AssignmentStatementAst] -and $_.Left.Extent.Text -eq '$report' })
Require ($init.Count -eq 1) 'Report must initialize exactly once'
$writes = @($nodes | Where-Object { ($_ -is [System.Management.Automation.Language.AssignmentStatementAst] -and $_.Left.Extent.Text.StartsWith('$report[')) -or ($_ -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $_.Member.Extent.Text -eq 'WriteAllText') })
foreach ($write in $writes) { Require ($init[0].Extent.EndOffset -lt $write.Extent.StartOffset) 'Report write precedes initialization' }
$waits = @($nodes | Where-Object { $_ -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $_.Member.Extent.Text -eq 'WaitForExit' })
Require ($waits.Count -eq 3) 'Unexpected process wait count'
foreach ($wait in $waits) { Require ($wait.Arguments.Count -eq 1 -and $wait.Arguments[0].Value -gt 0 -and $wait.Arguments[0].Value -le 60000) 'Unbounded helper wait' }
$kills = @($nodes | Where-Object { $_ -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $_.Member.Extent.Text -eq 'Kill' })
Require ($kills.Count -eq 1 -and $kills[0].Expression.Extent.Text -eq '$process') 'Only the retained helper process may be terminated'
$waitFunction = @($nodes | Where-Object { $_ -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -eq 'Wait-OwnedWatchdog' })
Require ($waitFunction.Count -eq 1) 'Missing retained watchdog wait'
. ([scriptblock]::Create($waitFunction[0].Extent.Text))
function Start-DisposableWaitChild([int]$Delay) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $PSHOME 'powershell.exe'
    $start.Arguments = "-NoProfile -NonInteractive -Command `"[Threading.Thread]::Sleep($Delay)`""
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    $child = [Diagnostics.Process]::Start($start); $null = $child.Handle
    return $child
}
foreach ($exited in @($false,$true)) {
    $child = Start-DisposableWaitChild 200
    try {
        $image = $child.Path; $birth = $child.StartTime.ToUniversalTime().ToFileTimeUtc()
        if ($exited) { Require ($child.WaitForExit(5000)) 'Disposable child did not exit' }
        Wait-OwnedWatchdog $child.Id $image $birth ([DateTime]::UtcNow.ToFileTimeUtc())
    } finally { if (-not $child.HasExited) { $child.Kill() }; $child.Dispose() }
}
$child = Start-DisposableWaitChild 5000
try {
    $birth = $child.StartTime.ToUniversalTime().ToFileTimeUtc(); $rejected = $false
    try { Wait-OwnedWatchdog $child.Id 'wrong-image.exe' $birth ([DateTime]::UtcNow.ToFileTimeUtc()) }
    catch { $rejected = $_.ToString().Contains('Watchdog process identity changed') }
    Require $rejected 'Wrong live process identity was accepted'
    $rejected = $false
    try { Wait-OwnedWatchdog $child.Id $child.Path ($birth + 1) ([DateTime]::UtcNow.ToFileTimeUtc()) }
    catch { $rejected = $_.ToString().Contains('Watchdog process identity changed') }
    Require $rejected 'Wrong process creation interval was accepted'
} finally { if (-not $child.HasExited) { $child.Kill() }; $child.Dispose() }
'Runner pins, fresh report, bounded waits, exited watchdog and wrong-identity checks passed'
