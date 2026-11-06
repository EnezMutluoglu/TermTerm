param(
    [Parameter(Mandatory=$true)][string]$Helper,
    [Parameter(Mandatory=$true)][string]$ExpectedHelperSha256,
    [ValidateSet('normal','broker-death')][string]$Mode = 'normal'
)
$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Explicit elevation is required; this script never requests UAC itself.' }
$helperPath = (Resolve-Path -LiteralPath $Helper).Path
if ([IO.Path]::GetFileName($helperPath) -ne 'termterm-vpn-broker.exe') { throw 'Unexpected helper filename' }
$bundle = Split-Path -Parent $helperPath
$helperLock = [IO.File]::Open($helperPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
try {
$hasher = [Security.Cryptography.SHA256]::Create()
try { $helperHash = ([BitConverter]::ToString($hasher.ComputeHash($helperLock))).Replace('-','').ToLowerInvariant() } finally { $hasher.Dispose() }
if ($helperHash -ne $ExpectedHelperSha256.ToLowerInvariant()) { $helperLock.Dispose(); throw 'Helper hash changed after approval' }
$driverHash = (Get-FileHash -LiteralPath (Join-Path $bundle 'wintun.dll') -Algorithm SHA256).Hash.ToLowerInvariant()
if ($driverHash -ne 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce') { throw 'Wintun hash mismatch' }

function Read-NetworkBaseline {
    $addresses = @(Get-NetIPAddress | Select-Object InterfaceIndex,AddressFamily,IPAddress,PrefixLength,PrefixOrigin,SuffixOrigin | Sort-Object InterfaceIndex,AddressFamily,IPAddress)
    $routes = @(Get-NetRoute | Select-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop,RouteMetric,Protocol | Sort-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop)
    $interfaces = @(Get-NetIPInterface | Select-Object InterfaceIndex,AddressFamily,NlMtu,Dhcp,RouterDiscovery,Forwarding | Sort-Object InterfaceIndex,AddressFamily)
    $dns = @(Get-DnsClientServerAddress | Select-Object InterfaceIndex,AddressFamily,@{Name='Servers';Expression={$_.ServerAddresses -join ','}} | Sort-Object InterfaceIndex,AddressFamily)
    [ordered]@{addresses=$addresses;routes=$routes;interfaces=$interfaces;dns=$dns}
}

$journalRoot = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'TermTermVpnDevBroker-v1'
if ((Test-Path -LiteralPath $journalRoot) -and @(Get-ChildItem -LiteralPath $journalRoot -Filter '*.json').Count) { throw 'Stale owned journals must be reviewed and recovered before this test' }
$baseline = Read-NetworkBaseline
$baselineJson = $baseline | ConvertTo-Json -Depth 8 -Compress
$baselinePath = Join-Path $bundle "privileged-$Mode-baseline.json"
$reportPath = Join-Path $bundle "privileged-$Mode-result.json"
[IO.File]::WriteAllText($baselinePath,$baselineJson)
$report = [ordered]@{mode=$Mode;helperSha256=$helperHash;wintunSha256=$driverHash;ok=$false;driverMayRemainInstalled=$true;realVpnAuthenticationTested=$false;kernelPacketExchangeTested=$false}
try {
    $arguments = @('--privileged-self-test')
    if ($Mode -eq 'broker-death') { $arguments += @('--fault','broker-death') }
    $lines = @(& $helperPath @arguments 2>&1 | ForEach-Object {"$_"})
    $exit = $LASTEXITCODE
    $report['helperOutput'] = $lines
    if (($Mode -eq 'normal' -and $exit -ne 0) -or ($Mode -eq 'broker-death' -and $exit -ne 92)) { throw "Unexpected helper exit code: $exit" }
    $result = $lines[-1] | ConvertFrom-Json
    if ($Mode -eq 'normal' -and -not $result.ok) { throw 'Normal cleanup was not confirmed' }
    if ($Mode -eq 'broker-death' -and -not $result.armed) { throw 'Crash injection did not arm owned objects' }
    if (-not $result.watchdogPid) { throw 'Missing watchdog PID' }
    $watchdog = Get-Process -Id $result.watchdogPid -ErrorAction SilentlyContinue
    if ($watchdog) { Wait-Process -Id $result.watchdogPid -Timeout 20 -ErrorAction Stop }
    if (@(Get-ChildItem -LiteralPath $journalRoot -Filter '*.json').Count) { throw 'Watchdog left owned journals requiring cleanup' }
    $after = Read-NetworkBaseline
    if (($after | ConvertTo-Json -Depth 8 -Compress) -ne $baselineJson) { throw 'Address, route, interface MTU/settings or DNS baseline differs' }
    $report['ok'] = $true
    $report['addressRouteInterfaceDnsBaselineRestored'] = $true
    $report['ownedJournalsEmpty'] = $true
} catch {
    $report['error'] = $_.Exception.Message
    throw
} finally {
    [IO.File]::WriteAllText($reportPath,($report | ConvertTo-Json -Depth 8))
    Write-Output "Validation report: $reportPath"
}
} finally { $helperLock.Dispose() }
