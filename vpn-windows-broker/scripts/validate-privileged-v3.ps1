param(
    [Parameter(Mandatory=$true)][string]$Helper,
    [Parameter(Mandatory=$true)][string]$ExpectedHelperSha256,
    [ValidateSet('normal','broker-death','adapter-created','coexistence','coexistence-crash','nested-routes')][string]$Mode = 'normal'
)
$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Explicit elevation is required; this script never requests UAC itself.' }
$helperPath = (Resolve-Path -LiteralPath $Helper).Path
if ([IO.Path]::GetFileName($helperPath) -ne 'termterm-vpn-broker.exe') { throw 'Unexpected helper filename' }
$bundle = Split-Path -Parent $helperPath
$helperLock = [IO.File]::Open($helperPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
$driverLock = $null
try {
$hasher = [Security.Cryptography.SHA256]::Create()
try { $helperHash = ([BitConverter]::ToString($hasher.ComputeHash($helperLock))).Replace('-','').ToLowerInvariant() } finally { $hasher.Dispose() }
if ($helperHash -ne $ExpectedHelperSha256.ToLowerInvariant()) { throw 'Helper hash changed after approval' }
$driverLock = [IO.File]::Open((Join-Path $bundle 'wintun.dll'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
$hasher = [Security.Cryptography.SHA256]::Create()
try { $driverHash = ([BitConverter]::ToString($hasher.ComputeHash($driverLock))).Replace('-','').ToLowerInvariant() } finally { $hasher.Dispose() }
if ($driverHash -ne 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce') { throw 'Wintun hash mismatch' }

function Read-NetworkBaseline {
    $addresses = @(Get-NetIPAddress | Select-Object InterfaceIndex,AddressFamily,IPAddress,PrefixLength,PrefixOrigin,SuffixOrigin | Sort-Object InterfaceIndex,AddressFamily,IPAddress)
    $routes = @(Get-NetRoute | Select-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop,RouteMetric,Protocol | Sort-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop)
    $interfaces = @(Get-NetIPInterface | Select-Object InterfaceIndex,AddressFamily,NlMtu,Dhcp,RouterDiscovery,Forwarding | Sort-Object InterfaceIndex,AddressFamily)
    $dns = @(Get-DnsClientServerAddress | Select-Object InterfaceIndex,AddressFamily,@{Name='Servers';Expression={$_.ServerAddresses -join ','}} | Sort-Object InterfaceIndex,AddressFamily)
    [ordered]@{addresses=$addresses;routes=$routes;interfaces=$interfaces;dns=$dns}
}
function Wait-OwnedWatchdog([int]$ProcessId,[string]$ExpectedPath,[long]$StartedNotBefore,[long]$StartedNotAfter) {
    try { $owned = Get-Process -Id $ProcessId -ErrorAction Stop }
    catch { if ($_.FullyQualifiedErrorId -like 'NoProcessFoundForGivenId,*') { return }; throw }
    try {
        $null = $owned.Handle
        if ($owned.HasExited) { return }
        $created = $owned.StartTime.ToUniversalTime().ToFileTimeUtc()
        $image = $owned.Path
        if ($owned.HasExited) { return }
        if ($created -lt $StartedNotBefore -or $created -gt $StartedNotAfter -or $image -ine $ExpectedPath) { throw 'Watchdog process identity changed' }
        if (-not $owned.WaitForExit(20000)) { throw 'Owned watchdog cleanup exceeded its deadline' }
    } catch {
        $cause = $_.Exception.GetBaseException()
        if (($cause -is [InvalidOperationException] -or ($cause -is [ComponentModel.Win32Exception] -and $cause.NativeErrorCode -in @(87,299))) -and $owned.HasExited) { return }
        throw
    } finally { $owned.Dispose() }
}

$journalRoot = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'TermTermVpnDevBroker-v1'
$baselinePath = Join-Path $bundle "privileged-v3-$Mode-baseline.json"
$reportPath = Join-Path $bundle "privileged-v3-$Mode-result.json"
$report = [ordered]@{mode=$Mode;helperSha256=$helperHash;wintunSha256=$driverHash;ok=$false;driverMayRemainInstalled=$true;realVpnAuthenticationTested=$false;kernelPacketExchangeTested=$false}
$baselineJson = $null
try {
    if ((Test-Path -LiteralPath $journalRoot) -and @(Get-ChildItem -LiteralPath $journalRoot -Filter '*.json').Count) { throw 'Stale owned journals must be reviewed and recovered before this test' }
    $baseline = Read-NetworkBaseline
    $baselineJson = $baseline | ConvertTo-Json -Depth 8 -Compress
    [IO.File]::WriteAllText($baselinePath,$baselineJson)
    $nativeBeforeText = & $helperPath --wintun-sweep-preflight
    if ($LASTEXITCODE -ne 0) { throw 'Wintun orphan or legacy device preflight refused the host' }
    $nativeBefore = $nativeBeforeText | ConvertFrom-Json
    if ($nativeBefore.readOnly -isnot [bool] -or -not $nativeBefore.readOnly -or $nativeBefore.safe -isnot [bool] -or -not $nativeBefore.safe -or @($nativeBefore.registeredTermTermInterfaces).Count) { throw 'Invalid initial native Wintun inventory' }
    $driverFile = Join-Path $env:windir 'System32\drivers\wintun.sys'
    $driverFileHash = (Get-FileHash -LiteralPath $driverFile -Algorithm SHA256).Hash.ToLowerInvariant()
    $report['installedDriverSha256'] = $driverFileHash
    $report['nativeBefore'] = $nativeBefore
    $driverPreflight = & $helperPath --wintun-driver-preflight
    if ($LASTEXITCODE -ne 0) { throw 'Preinstalled Wintun driver policy refused the package set' }
    $driverPolicy = $driverPreflight | ConvertFrom-Json
    if ($driverPolicy.readOnly -isnot [bool] -or -not $driverPolicy.readOnly -or $driverPolicy.safe -isnot [bool] -or -not $driverPolicy.safe) { throw 'Invalid read-only driver preflight receipt' }
    $report['driverPreflight'] = $driverPolicy
    $arguments = @('--privileged-self-test')
    $crash = $Mode -in @('broker-death','adapter-created','coexistence-crash')
    $fault = if ($Mode -eq 'coexistence-crash') { 'broker-death' } else { $Mode }
    if ($crash) { $arguments += @('--fault',$fault) }
    if ($Mode -in @('coexistence','coexistence-crash')) { $arguments += @('--coexistence','retained') }
    if ($Mode -eq 'nested-routes') { $arguments += @('--routes','nested') }
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $helperPath
    $start.Arguments = $arguments -join ' '
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) { throw 'Helper process did not start' }
        $report['helperPid'] = $process.Id
        $report['helperStartedAt'] = $process.StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $timedOut = -not $process.WaitForExit(60000)
        if ($timedOut) {
            $report['helperTimedOut'] = $true
            $process.Kill()
            if (-not $process.WaitForExit(10000)) { throw 'The retained helper process did not exit after its deadline' }
        }
        $exit = $process.ExitCode
        $report['helperExitedAt'] = [DateTime]::UtcNow.ToFileTimeUtc().ToString()
        if (-not $stdoutTask.Wait(10000) -or -not $stderrTask.Wait(20000)) { throw 'Helper output did not close after the bounded process wait' }
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        $report['helperExitCode'] = $exit
        $report['helperStdout'] = $stdout
        $report['helperStderr'] = $stderr
        $lines = @($stdout -split '\r?\n' | Where-Object { $_.Length })
        if ($timedOut) { throw 'Helper exceeded its sixty-second deadline; exact owned helper terminated for watchdog cleanup' }
    } finally { $process.Dispose() }
    if ((-not $crash -and $exit -ne 0) -or ($crash -and $exit -ne 92)) { throw "Unexpected helper exit code: $exit" }
    $result = $lines[-1] | ConvertFrom-Json
    if (-not $crash -and -not $result.ok) { throw 'Normal cleanup was not confirmed' }
    if ($Mode -eq 'nested-routes' -and ($result.nestedRoutes -isnot [bool] -or -not $result.nestedRoutes -or $result.allOwnedRoutesVerified -isnot [bool] -or -not $result.allOwnedRoutesVerified)) { throw 'All nested route ownership readbacks after the broad adds were not confirmed' }
    if ($Mode -eq 'nested-routes' -and ($result.ownedOrAbsentRouteUndoProofs -isnot [bool] -or -not $result.ownedOrAbsentRouteUndoProofs)) { throw 'Owned NETMGMT or documented absence before every nested route undo was not confirmed' }
    if ($crash -and ($result.armed -isnot [bool] -or -not $result.armed -or $result.fault -ne $fault)) { throw 'Crash injection did not arm the expected owned objects' }
    if ($Mode -eq 'adapter-created' -and ($result.adapterOnlyIntent -isnot [bool] -or -not $result.adapterOnlyIntent)) { throw 'Early crash did not occur with a sole adapter intent' }
    if ($Mode -in @('coexistence','coexistence-crash') -and ($result.retainedHealthyAdapterTested -isnot [bool] -or -not $result.retainedHealthyAdapterTested)) { throw 'Retained healthy adapter was not verified' }
    if ($Mode -eq 'coexistence-crash' -and $result.ownedJournalCount -ne 4) { throw 'Coexistence crash did not arm all four owned journals' }
    $report['helperResult'] = $result
    if (-not $result.watchdogPid) { throw 'Missing watchdog PID' }
    Wait-OwnedWatchdog $result.watchdogPid $helperPath ([long]$report.helperStartedAt) ([long]$report.helperExitedAt)
    if (@(Get-ChildItem -LiteralPath $journalRoot -Filter '*.json').Count) { throw 'Watchdog left owned journals requiring cleanup' }
    $driverAfter = & $helperPath --wintun-driver-preflight
    if ($LASTEXITCODE -ne 0 -or (($driverAfter | ConvertFrom-Json).packages | ConvertTo-Json -Depth 4 -Compress) -ne ($driverPolicy.packages | ConvertTo-Json -Depth 4 -Compress)) { throw 'Compatible Wintun driver package set changed' }
    if ((Get-FileHash -LiteralPath $driverFile -Algorithm SHA256).Hash.ToLowerInvariant() -ne $driverFileHash) { throw 'Installed Wintun driver file changed' }
    $nativeAfterText = & $helperPath --wintun-sweep-preflight
    if ($LASTEXITCODE -ne 0) { throw 'Final native Wintun inventory is unsafe' }
    $nativeAfter = $nativeAfterText | ConvertFrom-Json
    $report['nativeAfter'] = $nativeAfter
    if (($nativeAfter | ConvertTo-Json -Depth 6 -Compress) -ne ($nativeBefore | ConvertTo-Json -Depth 6 -Compress)) { throw 'Native Wintun identities or device statuses changed' }
    $after = Read-NetworkBaseline
    if (($after | ConvertTo-Json -Depth 8 -Compress) -ne $baselineJson) { throw 'Address, route, interface MTU/settings or DNS baseline differs' }
    $report['ok'] = $true
    $report['addressRouteInterfaceDnsBaselineRestored'] = $true
    $report['ownedJournalsEmpty'] = $true
} catch {
    $report['error'] = $_.ToString()
    throw
} finally {
    try {
        $remainingWatchdogs = @()
        if ($report.Contains('helperPid')) {
            $remainingWatchdogs = @(Get-CimInstance Win32_Process -Filter "Name='termterm-vpn-broker.exe'" | Where-Object { $_.ParentProcessId -eq $report.helperPid } | Select-Object ProcessId,ParentProcessId,CreationDate)
            foreach ($remaining in $remainingWatchdogs) { Wait-OwnedWatchdog $remaining.ProcessId $helperPath ([long]$report.helperStartedAt) ([DateTime]::UtcNow.ToFileTimeUtc()) }
        }
        $report['postFailureWatchdogsObserved'] = $remainingWatchdogs
        $final = Read-NetworkBaseline
        $report['finalAddressRouteInterfaceDnsBaselineRestored'] = (($final | ConvertTo-Json -Depth 8 -Compress) -eq $baselineJson)
        $report['finalTermTermAdapters'] = @(Get-NetAdapter -IncludeHidden | Where-Object {$_.Name -like 'TermTerm VPN *'} | Select-Object Name,InterfaceGuid,InterfaceIndex,Status)
        $report['finalOwnedJournalFiles'] = @(Get-ChildItem -LiteralPath $journalRoot -Force | Select-Object Name,Length,Attributes)
        $report['finalWintunDrivers'] = @(Get-CimInstance Win32_SystemDriver -Filter "Name='Wintun'" | Select-Object Name,State,StartMode)
        if (-not $report.finalAddressRouteInterfaceDnsBaselineRestored -or @($report.finalOwnedJournalFiles).Count) { $report['ok'] = $false }
    } catch { $report['postFailureDiagnosticError'] = $_.ToString(); $report['ok'] = $false }
    [IO.File]::WriteAllText($reportPath,($report | ConvertTo-Json -Depth 8))
    Write-Output "Validation report: $reportPath"
}
} finally { if ($driverLock) { $driverLock.Dispose() }; $helperLock.Dispose() }


