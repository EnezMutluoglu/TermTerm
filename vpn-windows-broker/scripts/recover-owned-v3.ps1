param([Parameter(Mandatory=$true)][string]$ExpectedHelperSha256,[switch]$ThenLifecycleV6)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$security = [Security.AccessControl.DirectorySecurity]::new()
$security.SetAccessRuleProtection($true,$false)
$security.SetOwner($identity.User)
foreach ($sid in @($identity.User.Value,'S-1-5-18','S-1-5-32-544') | Select-Object -Unique) {
    $security.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow))
}
$output = Join-Path $workspace ('.tools/vpn-windows/recovery-v3-result-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff') + '-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($output,$security) | Out-Null
$report = [ordered]@{ok=$false;stage='created';output=$output;lifecycleV6Requested=[bool]$ThenLifecycleV6}
$root = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'TermTermVpnDevBroker-v1'
$expectedSet = 'a89a826a23729a0cbbaa9761e6c413c4ae99cb3f734a60e2d3526965e21c3656'
$locks = @()

function Lock-Image([string]$Path,[string]$Expected) {
    $stream = [IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $hash=([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
        if ($hash -ne $Expected.ToLowerInvariant()) { throw "Approved file hash changed: $Path" }
        return $stream
    } catch { $stream.Dispose(); throw }
}
function Read-NetworkState {
    [ordered]@{
        addresses=@(Get-NetIPAddress | Select-Object InterfaceIndex,AddressFamily,IPAddress,PrefixLength,PrefixOrigin,SuffixOrigin | Sort-Object InterfaceIndex,AddressFamily,IPAddress)
        routes=@(Get-NetRoute | Select-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop,RouteMetric,Protocol | Sort-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop)
        interfaces=@(Get-NetIPInterface | Select-Object InterfaceIndex,AddressFamily,NlMtu,Dhcp,RouterDiscovery,Forwarding | Sort-Object InterfaceIndex,AddressFamily)
        dns=@(Get-DnsClientServerAddress | Select-Object InterfaceIndex,AddressFamily,@{Name='Servers';Expression={$_.ServerAddresses -join ','}} | Sort-Object InterfaceIndex,AddressFamily)
    }
}
function Read-JournalState {
    if ((Get-Item -LiteralPath $root -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Protected root is a reparse point' }
    if ((Get-Acl -LiteralPath $root).Sddl -ne 'O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)') { throw 'Protected root ACL changed' }
    $files=@(Get-ChildItem -LiteralPath $root -Force)
    if ($files.Count -gt 3) { throw 'Unexpected protected journal count' }
    @($files | Sort-Object Name | ForEach-Object {
        if ($_.PSIsContainer -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $_.Length -gt 2097152 -or $_.Extension -ne '.json') { throw 'Unexpected protected journal file' }
        $acl=(Get-Acl -LiteralPath $_.FullName).Sddl
        if ($acl -ne 'O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)') { throw 'Protected journal ACL changed' }
        $journal=Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json
        [ordered]@{name=$_.Name;bytes=$_.Length;sddl=$acl;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant();guids=@($journal.entries | ForEach-Object {$_.object.guid} | Sort-Object -Unique)}
    })
}
function Invoke-Captured([string]$Name,[string]$File,[string]$Arguments,[bool]$AllowFailure=$false) {
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$File; $start.Arguments=$Arguments; $start.UseShellExecute=$false; $start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$start
    try {
        if (-not $process.Start()) { throw "$Name did not start" }
        $report[$Name+'Pid']=$process.Id
        $stdout=$process.StandardOutput.ReadToEndAsync(); $stderr=$process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(90000)) { throw "$Name still running after 90 seconds; no process terminated. Inspect PID before retry." }
        $out=$stdout.GetAwaiter().GetResult(); $err=$stderr.GetAwaiter().GetResult()
        [IO.File]::WriteAllText((Join-Path $output ($Name+'-stdout.log')),$out)
        [IO.File]::WriteAllText((Join-Path $output ($Name+'-stderr.log')),$err)
        $report[$Name+'ExitCode']=$process.ExitCode
        if ($process.ExitCode -ne 0 -and -not $AllowFailure) { throw "$Name exit $($process.ExitCode); stdout/stderr preserved" }
        return $out
    } finally { $process.Dispose() }
}
try {
    $report['stage']='require-elevation'
    if (-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Explicit elevation required; this runner never requests UAC.' }
    $report['stage']='pin-approved-files'
    $helper=Join-Path $workspace '.tools/vpn-windows/broker-validation-v6/termterm-vpn-broker.exe'
    $locks+=Lock-Image $helper $ExpectedHelperSha256
    $locks+=Lock-Image (Join-Path (Split-Path -Parent $helper) 'wintun.dll') 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce'
    $report['helperSha256']=$ExpectedHelperSha256.ToLowerInvariant(); $report['expectedJournalSetSha256']=$expectedSet
    $report['stage']='capture-before'
    $report['journalsBefore']=@(Read-JournalState)
    $lines=@($report.journalsBefore | ForEach-Object {$_.name+':'+$_.sha256})
    $sha=[Security.Cryptography.SHA256]::Create()
    try { $actualSet=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($lines -join "`n")+"`n")))).Replace('-','').ToLowerInvariant() } finally {$sha.Dispose()}
    if ($actualSet -ne $expectedSet) { throw 'Expected journal identity/hash set changed; no recovery invoked' }
    $report['baseline']=Read-NetworkState
    $baselineJson=$report.baseline | ConvertTo-Json -Depth 8 -Compress
    if (@(Get-CimInstance Win32_Process -Filter "Name='termterm-vpn-broker.exe'").Count) { throw 'Existing broker process; no recovery invoked' }
    $report['stage']='recover-absent-owned-journals'
    $stdout=Invoke-Captured 'recovery' $helper ('--recover-owned-journals --expected-set-sha256 '+$expectedSet)
    $receipt=(@($stdout -split '\r?\n' | Where-Object {$_.Length})[-1]) | ConvertFrom-Json
    $report['recoveryReceipt']=$receipt
    if (-not $receipt.ok -or $receipt.recoveredJournals -ne 3 -or $receipt.remainingJournals -ne 0 -or $receipt.expectedSetSha256 -ne $expectedSet) { throw 'Recovery receipt incomplete' }
    $report['journalsAfterRecovery']=@(Read-JournalState)
    $report['afterRecovery']=Read-NetworkState
    if ($report.journalsAfterRecovery.Count -or (($report.afterRecovery | ConvertTo-Json -Depth 8 -Compress) -ne $baselineJson)) { throw 'Recovery did not leave empty journals and unchanged network baseline' }
    $native=(Invoke-Captured 'post-recovery-native-inventory' $helper '--wintun-sweep-preflight' $true) | ConvertFrom-Json
    $report['nativeAfterRecovery']=$native
    $ownedGuids=@($report.journalsBefore | ForEach-Object {$_.guids} | Sort-Object -Unique)
    foreach ($guid in $ownedGuids) {
        if (@($native.registeredTermTermInterfaces | Where-Object {$_.guid -eq $guid}).Count -or @($native.devices | Where-Object {$_.instance -eq ('SWD\WINTUN\{'+$guid+'}')}).Count) {throw 'Recovered journal GUID or exact devnode still present'}
    }
    $report['recoveredGuidAndDevnodeAbsenceVerified']=$true
    $report['recoveryVerified']=$true
    if ($ThenLifecycleV6) {
        $runner=Join-Path $PSScriptRoot 'validate-privileged-v3.ps1'
        $locks+=Lock-Image $runner 'ed7aa11bd344ed06f3a6b9299c18dc7e6bf7ccee81157e230be7a97a403bab77'
        foreach ($mode in @('normal','broker-death')) {
            $report['stage']='lifecycle-v6-'+$mode
            $sweep=(Invoke-Captured ('pre-'+$mode+'-sweep') $helper '--wintun-sweep-preflight') | ConvertFrom-Json
            $report['pre-'+$mode+'Sweep']=$sweep
            if (-not $sweep.safe -or @($sweep.registeredTermTermInterfaces).Count) {throw 'Native Wintun sweep/owned identity preflight refused broad phase'}
            foreach ($suffix in @('baseline','result')) {
                if (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $helper) ('privileged-v3-'+$mode+'-'+$suffix+'.json'))) {throw 'Existing lifecycle v6 evidence would be overwritten; refusing'}
            }
            $powerShell=Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
            $null=Invoke-Captured ('v6-'+$mode) $powerShell ('-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "'+$runner+'" -Helper "'+$helper+'" -ExpectedHelperSha256 '+$ExpectedHelperSha256+' -Mode '+$mode)
            $phase=Get-Content -Raw -LiteralPath (Join-Path (Split-Path -Parent $helper) ('privileged-v3-'+$mode+'-result.json')) | ConvertFrom-Json
            $report['v6-'+$mode+'Report']=$phase
            if (-not $phase.ok) {throw ('Frozen runner v6 '+$mode+' report did not pass')}
            if (@(Read-JournalState).Count -or ((Read-NetworkState | ConvertTo-Json -Depth 8 -Compress) -ne $baselineJson)) {throw ('Lifecycle v6 '+$mode+' did not leave empty journals and restored baseline')}
            $sweep=(Invoke-Captured ('post-'+$mode+'-sweep') $helper '--wintun-sweep-preflight') | ConvertFrom-Json
            $report['post-'+$mode+'Sweep']=$sweep
            if (-not $sweep.safe -or @($sweep.registeredTermTermInterfaces).Count) {throw 'Native Wintun devnode or raw owned identity remains after phase'}
        }
    }
    $report['ok']=$true; $report['stage']='complete'
} catch {
    $report['error']=$_.ToString(); $report['exception']=$_.Exception.ToString(); $report['position']=$_.InvocationInfo.PositionMessage; $report['scriptStack']=$_.ScriptStackTrace
} finally {
    try {
        if ($report.Contains('baseline')) {
            $report['journalsFinal']=@(Read-JournalState); $report['networkFinal']=Read-NetworkState
            $report['finalBaselineRestored']=(($report.networkFinal | ConvertTo-Json -Depth 8 -Compress) -eq $baselineJson)
            $report['brokersFinal']=@(Get-CimInstance Win32_Process -Filter "Name='termterm-vpn-broker.exe'" | Select-Object ProcessId,ParentProcessId,CreationDate)
            if (-not $report.finalBaselineRestored -or $report.journalsFinal.Count -or $report.brokersFinal.Count) {$report['ok']=$false}
        }
    } catch {$report['finalDiagnosticError']=$_.ToString();$report['ok']=$false}
    foreach ($lock in $locks) {$lock.Dispose()}
    [IO.File]::WriteAllText((Join-Path $output 'recovery-result.json'),($report | ConvertTo-Json -Depth 12))
    Write-Output $output
}
if (-not $report.ok) {exit 1}
