$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Read-only journal diagnostics require explicit elevation' }
$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$bundle = Join-Path $workspace '.tools/vpn-windows/broker-validation'
$root = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'TermTermVpnDevBroker-v1'
$report = [ordered]@{readOnly=$true;rootExists=$false;journals=@();error=$null}
try {
    $report['rootExists'] = Test-Path -LiteralPath $root
    if ($report.rootExists) {
        $item = Get-Item -LiteralPath $root -Force
        $report['rootAttributes'] = "$($item.Attributes)"
        $report['rootSddl'] = (Get-Acl -LiteralPath $root).Sddl
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Refusing to inspect a reparse-point journal directory' }
        $report['journals'] = @(Get-ChildItem -LiteralPath $root -Force | ForEach-Object {
            [ordered]@{name=$_.Name;bytes=$_.Length;attributes="$($_.Attributes)";sddl=(Get-Acl -LiteralPath $_.FullName).Sddl}
        })
    }
    $report['termTermAdapters'] = @(Get-NetAdapter -IncludeHidden | Where-Object {$_.Name -like 'TermTerm VPN *'} | Select-Object Name,InterfaceGuid,InterfaceIndex,Status)
    $report['wintunSystemDrivers'] = @(Get-CimInstance Win32_SystemDriver -Filter "Name='Wintun'" | Select-Object Name,State,StartMode,PathName)
} catch { $report['error'] = $_.ToString() }
$path = Join-Path $bundle 'privileged-diagnostic-v1.json'
[IO.File]::WriteAllText($path,($report | ConvertTo-Json -Depth 8))
Write-Output $path
