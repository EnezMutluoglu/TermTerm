param(
    [string]$PreviousInstaller,
    [string]$PreviousStage,
    [string]$ExpectedPreviousInstallerSha256,
    [string]$Installer,
    [string]$Stage,
    [string]$ExpectedInstallerSha256,
    [switch]$SelfTest
)
$ErrorActionPreference='Stop'
function Require($Value,[string]$Message) {if (-not $Value) {throw $Message}}
function Assert-PlainPath([string]$Path) {
    $current=[IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {Require (-not ((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) 'Reparse paths are not accepted'}
        $parent=Split-Path $current -Parent
        if ($parent -ceq $current) {break};$current=$parent
    }
}
function Assert-NewDirectory([string]$Path,[string]$Parent) {
    $full=[IO.Path]::GetFullPath($Path);$prefix=[IO.Path]::GetFullPath($Parent).TrimEnd('\')+'\'
    Require ($full.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase) -and -not (Test-Path -LiteralPath $full)) 'Use a fresh contained test directory; preserve existing data'
    Assert-PlainPath $full
}
function Assert-Upgrade([string]$Previous,[string]$Current) {
    $pattern='^(\d+\.\d+\.\d+)-dev\.(0|[1-9][0-9]{0,8})$'
    Require ($Previous -cmatch $pattern) 'Previous package must be a development version';$base=$Matches[1];$old=[int]$Matches[2]
    Require ($Current -cmatch $pattern) 'Current package must be a development version'
    Require ($Matches[1] -ceq $base -and [int]$Matches[2] -gt $old) 'Require an increasing dev.N version on the same base; reinstall is not upgrade coverage'
}
function Assert-Manifest($Manifest) {
    Require ($Manifest.version -eq 1 -and $Manifest.applicationIdentifier -ceq 'local.termterm.desktop.vpndev' -and $Manifest.developmentOnly -is [bool] -and $Manifest.developmentOnly) 'Only the isolated VPN development bundle is accepted'
    $files=@($Manifest.files.PSObject.Properties)
    Require ($files.Count -ge 3 -and $files.Count -le 64) 'Invalid VPN bundle inventory'
    foreach ($name in @('termterm-vpn-dev.exe','termterm-vpn-broker.exe','wintun.dll')) {Require ($name -cin $files.Name) 'Required development bundle member missing'}
    foreach ($file in $files) {
        Require ($file.Name -cmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$' -and $file.Name -cne 'termterm.exe' -and $file.Name -notmatch '^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)' -and $file.Value -cmatch '^[0-9a-f]{64}$') 'Unsafe bundle filename or hash'
    }
}
function File-Hash([string]$Path) {(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
function Read-Stage([string]$Directory,[string]$Setup,[string]$Expected,[string]$Tools,[string]$Project) {
    $directoryPath=(Resolve-Path -LiteralPath $Directory).Path;$setupPath=(Resolve-Path -LiteralPath $Setup).Path
    Require ($directoryPath.StartsWith($Tools,[StringComparison]::OrdinalIgnoreCase) -and $setupPath.StartsWith($Tools,[StringComparison]::OrdinalIgnoreCase)) 'Use reviewed local VPN packages under this worktree .tools/vpn-windows'
    Assert-PlainPath $directoryPath;Assert-PlainPath $setupPath
    Require ($Expected -cmatch '^[0-9a-f]{64}$' -and (File-Hash $setupPath) -ceq $Expected) 'Reviewed installer SHA256 differs'
    $manifest=Get-Content -LiteralPath (Join-Path $directoryPath 'vpn-dev-bundle.json') -Raw|ConvertFrom-Json
    Assert-Manifest $manifest
    foreach ($file in $manifest.files.PSObject.Properties) {
        $path=Join-Path $directoryPath $file.Name;Assert-PlainPath $path
        Require ((Test-Path -LiteralPath $path -PathType Leaf) -and (File-Hash $path) -ceq $file.Value) 'Staged bundle bytes differ'
    }
    $info=(Get-Item -LiteralPath (Join-Path $directoryPath 'termterm-vpn-dev.exe')).VersionInfo
    Require ($info.ProductName -ceq 'TermTerm VPN Dev') 'Wrong desktop product identity'
    $null=& python (Join-Path $Project 'scripts/stage-vpn-windows-dev.py') --verify-installer $setupPath --destination $directoryPath
    Require ($LASTEXITCODE -eq 0) 'Actual installer payload differs from its NSIS-specific stage'
    return @{directory=$directoryPath;installer=$setupPath;version=$info.ProductVersion;manifest=$manifest}
}
function Verify-Installed([string]$Target,$Package) {
    foreach ($file in $Package.manifest.files.PSObject.Properties) {Require ((File-Hash (Join-Path $Target $file.Name)) -ceq $file.Value) 'Installed VPN payload differs from reviewed bytes'}
    $registration=Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TermTerm VPN Dev'
    Require ($registration.DisplayName -ceq 'TermTerm VPN Dev' -and $registration.DisplayVersion -ceq $Package.version -and $registration.InstallLocation.Trim('"') -ceq $Target -and $registration.MainBinaryName -ceq 'termterm-vpn-dev.exe') 'Installed identity, version or location differs'
}
function Run-Setup([string]$Program,[string[]]$Arguments) {
    $process=Start-Process -FilePath $Program -ArgumentList $Arguments -WindowStyle Hidden -PassThru
    $null=$process.Handle
    try {Require ($process.WaitForExit(180000) -and $process.ExitCode -eq 0) 'Owned installer/uninstaller did not exit successfully'}
    finally {if (-not $process.HasExited) {$process.Kill();$null=$process.WaitForExit(5000)};$process.Dispose()}
}
function Window-Diagnostic([int]$ProcessId,[double]$ElapsedMs,[bool]$Exited,$ExitCode,[long]$Hwnd,[double]$TimeoutMs=30000) {
    $reason=if ($Exited) {if ($ExitCode -eq 0) {'desktop_early_exit_zero'} else {'desktop_early_exit_nonzero'}} elseif ($Hwnd -ne 0) {'window_observed'} elseif ($ElapsedMs -ge $TimeoutMs) {'window_timeout'} else {'pending'}
    return [ordered]@{pid=$ProcessId;elapsedMs=[math]::Round($ElapsedMs);observedHwnd=$Hwnd;earlyExitCode=$(if ($Exited) {$ExitCode} else {$null});reasonCode=$reason}
}
function Check-Window([string]$Program,[string]$Case,[ValidateSet('previous','current')][string]$Label) {
    $stdout=Join-Path $Case ($Label+'-desktop.stdout.log');$stderr=Join-Path $Case ($Label+'-desktop.stderr.log');$record=Join-Path $Case ($Label+'-desktop.launch.json')
    foreach ($path in @($stdout,$stderr,$record)) {Assert-PlainPath $path;Require (-not (Test-Path -LiteralPath $path)) 'Preserve existing launch diagnostics'}
    $watch=[Diagnostics.Stopwatch]::StartNew();$process=$null;$diagnostic=Window-Diagnostic 0 0 $false $null 0
    $diagnostic.reasonCode='launch_failed';$diagnostic['stdoutFile']=$stdout;$diagnostic['stderrFile']=$stderr;$diagnostic['forcedTermination']=$false
    try {
        $process=Start-Process -FilePath $Program -WindowStyle Normal -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
        $diagnostic.pid=$process.Id
        $null=$process.Handle
        do {
            $process.Refresh();$exitCode=if ($process.HasExited) {$process.ExitCode} else {$null}
            $state=Window-Diagnostic $process.Id $watch.Elapsed.TotalMilliseconds $process.HasExited $exitCode $process.MainWindowHandle.ToInt64()
            foreach ($key in $state.Keys) {$diagnostic[$key]=$state[$key]}
            if ($diagnostic.reasonCode -cne 'pending') {break};Start-Sleep -Milliseconds 200
        } while ($true)
        Require ($diagnostic.reasonCode -ceq 'window_observed') $diagnostic.reasonCode
        if (@(Get-NetTCPConnection -State Listen -OwningProcess $process.Id -ErrorAction SilentlyContinue|Where-Object LocalPort -eq 4445)) {$diagnostic.reasonCode='native_test_driver_listener';throw $diagnostic.reasonCode}
        $diagnostic.reasonCode='window_close_rejected';Require ($process.CloseMainWindow()) $diagnostic.reasonCode
        $diagnostic.reasonCode='window_close_timeout';Require ($process.WaitForExit(15000)) $diagnostic.reasonCode
        $diagnostic['exitAfterCloseCode']=$process.ExitCode;$diagnostic.reasonCode='window_close_nonzero_exit';Require ($process.ExitCode -eq 0) $diagnostic.reasonCode
        $diagnostic.reasonCode='gracefully_closed'
        return $diagnostic
    } catch {
        $diagnostic['errorType']=$_.Exception.GetType().FullName
        if ($diagnostic.reasonCode -cin @('pending','window_observed')) {$diagnostic.reasonCode='launch_observation_error'}
        throw $diagnostic.reasonCode
    } finally {
        if ($process) {if (-not $process.HasExited) {$diagnostic.forcedTermination=$true;$process.Kill();$null=$process.WaitForExit(5000)};if ($process.HasExited) {$diagnostic['exitCodeAfterOwnedCleanup']=$process.ExitCode};$process.Dispose()}
        $watch.Stop();$diagnostic.elapsedMs=[math]::Round($watch.Elapsed.TotalMilliseconds)
        [IO.File]::WriteAllText($record,($diagnostic|ConvertTo-Json -Depth 4))
    }
}
function Read-StableInstallation {
    $snapshot=[ordered]@{}
    foreach ($hive in @('HKCU','HKLM')) {foreach ($name in @('TermTerm','local.termterm.desktop')) {
        $key=$hive+':\Software\Microsoft\Windows\CurrentVersion\Uninstall\'+$name
        if (Test-Path -LiteralPath $key) {
            $value=Get-ItemProperty -LiteralPath $key;$snapshot[$key]=@($value.DisplayVersion,$value.InstallLocation,$value.UninstallString)
            if ($value.InstallLocation) {$binary=Join-Path $value.InstallLocation.Trim('"') 'termterm.exe';if (Test-Path -LiteralPath $binary -PathType Leaf) {$snapshot[$binary]=File-Hash $binary}}
        }
    }}
    foreach ($path in @((Join-Path ([Environment]::GetFolderPath('Desktop')) 'TermTerm.lnk'),(Join-Path ([Environment]::GetFolderPath('Programs')) 'TermTerm.lnk'),(Join-Path ([Environment]::GetFolderPath('Programs')) 'TermTerm/TermTerm.lnk'))) {$snapshot[$path]=if (Test-Path -LiteralPath $path) {File-Hash $path} else {$null}}
    return ($snapshot|ConvertTo-Json -Depth 5 -Compress)
}
function Save-DataRecovery([string[]]$Data,[string]$Case,$Moved,$Prepared) {
    $state=for ($i=0;$i -lt $Data.Count;$i++) {[ordered]@{path=$Data[$i];backup=(Join-Path $Case ('original-data-'+$i));captured=(Join-Path $Case ('captured-test-data-'+$i));originalMoved=$Moved.ContainsKey($Data[$i]);prepared=$Prepared.Contains($Data[$i])}}
    [IO.File]::WriteAllText((Join-Path $Case 'data-recovery.json'),(ConvertTo-Json -InputObject @($state) -Depth 4))
}
function Prepare-TestData([string[]]$Data,[string]$Case,$Moved,$Prepared) {
    Save-DataRecovery $Data $Case $Moved $Prepared
    for ($i=0;$i -lt $Data.Count;$i++) {
        Assert-PlainPath $Data[$i]
        if (Test-Path -LiteralPath $Data[$i]) {Require (Test-Path -LiteralPath $Data[$i] -PathType Container) 'Development data path must be a directory';$backup=Join-Path $Case ('original-data-'+$i);[IO.Directory]::Move($Data[$i],$backup);$Moved[$Data[$i]]=$backup}
        $null=$Prepared.Add($Data[$i]);Save-DataRecovery $Data $Case $Moved $Prepared
        $null=New-Item -ItemType Directory -Path $Data[$i]
        [IO.File]::WriteAllText((Join-Path $Data[$i] 'installer-acceptance-sentinel'),'preserve-disposable-development-data')
    }
}
function Restore-TestData([string[]]$Data,[string]$Case,$Moved,$Prepared) {
    # Check every capture destination first: a failed restore must preserve both originals and test evidence.
    for ($i=0;$i -lt $Data.Count;$i++) {if ($Prepared.Contains($Data[$i])) {Assert-PlainPath $Data[$i];Assert-NewDirectory (Join-Path $Case ('captured-test-data-'+$i)) $Case;if ($Moved.ContainsKey($Data[$i])) {Assert-PlainPath $Moved[$Data[$i]];Require (Test-Path -LiteralPath $Moved[$Data[$i]] -PathType Container) 'Original data backup missing; preserve recovery record'}}}
    for ($i=0;$i -lt $Data.Count;$i++) {
        if (-not $Prepared.Contains($Data[$i])) {continue}
        if (Test-Path -LiteralPath $Data[$i]) {[IO.Directory]::Move($Data[$i],(Join-Path $Case ('captured-test-data-'+$i)))}
        if ($Moved.ContainsKey($Data[$i])) {[IO.Directory]::Move($Moved[$Data[$i]],$Data[$i]);$Moved.Remove($Data[$i])}
        $null=$Prepared.Remove($Data[$i]);Save-DataRecovery $Data $Case $Moved $Prepared
    }
}
if ($SelfTest) {
    Assert-Upgrade '0.4.3-dev.1' '0.4.3-dev.2';$checks=1
    foreach ($pair in @(@('0.4.3-dev.1','0.4.3-dev.1'),@('0.4.3-dev.2','0.4.3-dev.1'),@('0.4.3','0.4.3-dev.2'),@('0.4.3-dev.1','0.4.5-dev.2'))) {$failed=$false;try {Assert-Upgrade $pair[0] $pair[1]} catch {$failed=$true};Require $failed 'Unsafe upgrade accepted';$checks++}
    $temporary=Join-Path ([IO.Path]::GetTempPath()) ('termterm-vpn-installer-selftest-'+[Guid]::NewGuid().ToString('N'))
    $null=New-Item -ItemType Directory -Path $temporary;$existing=Join-Path $temporary 'existing';$null=New-Item -ItemType Directory -Path $existing
    [IO.File]::WriteAllText((Join-Path $existing 'preserved'),'keep')
    Assert-NewDirectory (Join-Path $temporary 'fresh') $temporary
    foreach ($path in @($existing,(Join-Path $temporary '../outside'))) {$failed=$false;try {Assert-NewDirectory $path $temporary} catch {$failed=$true};Require $failed 'Existing or escaping directory accepted';$checks++}
    Require ([IO.File]::ReadAllText((Join-Path $existing 'preserved')) -ceq 'keep') 'Existing fixture changed'
    $manifest=[pscustomobject]@{version=1;applicationIdentifier='local.termterm.desktop.vpndev';developmentOnly=$true;files=[pscustomobject]@{'termterm-vpn-dev.exe'=('a'*64);'termterm-vpn-broker.exe'=('b'*64);'wintun.dll'=('c'*64)}}
    Assert-Manifest $manifest;$manifest.applicationIdentifier='local.termterm.desktop';$failed=$false;try {Assert-Manifest $manifest} catch {$failed=$true};Require $failed 'Stable identity accepted';$checks++
    $manifest.applicationIdentifier='local.termterm.desktop.vpndev';$manifest.files|Add-Member -NotePropertyName '../escape.dll' -NotePropertyValue ('d'*64);$failed=$false;try {Assert-Manifest $manifest} catch {$failed=$true};Require $failed 'Traversal accepted';$checks++
    $fixture=Join-Path $temporary 'inert-exit.ps1'
    [IO.File]::WriteAllLines($fixture,@('param([int]$ExitCode)','[Console]::Out.WriteLine("inert-startup")','[Console]::Error.WriteLine("inert-startup-error")','exit $ExitCode'))
    foreach ($code in @(0,17)) {
        $out=Join-Path $temporary ('exit-'+$code+'.stdout');$err=Join-Path $temporary ('exit-'+$code+'.stderr');$watch=[Diagnostics.Stopwatch]::StartNew()
        $p=Start-Process -FilePath powershell.exe -ArgumentList @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',('"'+$fixture+'"'),$code) -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError $err -PassThru;$null=$p.Handle
        try {Require ($p.WaitForExit(5000) -and $p.ExitCode -eq $code) 'Native exit code lost';$p.WaitForExit();$state=Window-Diagnostic $p.Id $watch.Elapsed.TotalMilliseconds $p.HasExited $p.ExitCode 0;Require ($state.reasonCode -ceq $(if ($code -eq 0) {'desktop_early_exit_zero'} else {'desktop_early_exit_nonzero'}) -and $state.earlyExitCode -eq $code -and $state.pid -gt 0 -and $state.elapsedMs -ge 0 -and [IO.File]::ReadAllText($out).Trim() -ceq 'inert-startup' -and [IO.File]::ReadAllText($err).Trim() -ceq 'inert-startup-error') 'Early-exit diagnostic or redirected output lost'} finally {if (-not $p.HasExited) {$p.Kill();$null=$p.WaitForExit(5000)};$p.Dispose();$watch.Stop()};$checks++
    }
    $p=Start-Process -FilePath powershell.exe -ArgumentList @('-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 5') -WindowStyle Hidden -PassThru;$null=$p.Handle
    try {$state=Window-Diagnostic $p.Id 1 $p.HasExited $null 0 0;Require ($state.reasonCode -ceq 'window_timeout' -and $state.earlyExitCode -eq $null -and $state.observedHwnd -eq 0 -and $state.pid -eq $p.Id) 'Windowless timeout diagnostic lost'} finally {if (-not $p.HasExited) {$p.Kill();$null=$p.WaitForExit(5000)};$p.Dispose()};$checks++
    $case=Join-Path $temporary 'partial';$null=New-Item -ItemType Directory -Path $case
    $data=@((Join-Path $temporary 'first-original'),(Join-Path $temporary 'second-original'))
    $null=New-Item -ItemType Directory -Path $data[0];[IO.File]::WriteAllText((Join-Path $data[0] 'private-fixture'),'original-first')
    # A file at the second directory path fails after the first actual rename; it must remain untouched.
    [IO.File]::WriteAllText($data[1],'original-second')
    $moved=@{};$prepared=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $failed=$false;try {Prepare-TestData $data $case $moved $prepared} catch {$failed=$true}
    Require ($failed -and $moved.Count -eq 1 -and $prepared.Count -eq 1) 'Partial preparation failure not exercised'
    $collision=Join-Path $case 'captured-test-data-0';$null=New-Item -ItemType Directory -Path $collision
    $failed=$false;try {Restore-TestData $data $case $moved $prepared} catch {$failed=$true}
    Require ($failed -and [IO.File]::ReadAllText((Join-Path $moved[$data[0]] 'private-fixture')) -ceq 'original-first' -and (Test-Path -LiteralPath (Join-Path $data[0] 'installer-acceptance-sentinel')) -and [IO.File]::ReadAllText($data[1]) -ceq 'original-second') 'Failed recovery changed an original or lost test evidence';$checks++
    [IO.Directory]::Move($collision,(Join-Path $case 'preserved-collision'))
    Restore-TestData $data $case $moved $prepared
    Require ([IO.File]::ReadAllText((Join-Path $data[0] 'private-fixture')) -ceq 'original-first' -and [IO.File]::ReadAllText($data[1]) -ceq 'original-second' -and $prepared.Count -eq 0 -and $moved.Count -eq 0 -and (Test-Path -LiteralPath (Join-Path $collision 'installer-acceptance-sentinel'))) 'Partial preparation did not restore the exact originals';$checks++
    [ordered]@{selfTest=$true;passed=$checks;installerExecuted=$false;applicationDataMoved=$false;networkActions=$false}|ConvertTo-Json -Compress
    return
}
Require ($PreviousInstaller -and $PreviousStage -and $ExpectedPreviousInstallerSha256 -and $Installer -and $Stage -and $ExpectedInstallerSha256) 'Two reviewed installer/stage pairs and SHA256 pins are required'
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
Require (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) 'Run the current-user installer check unelevated'
$project=(Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path;$tools=Join-Path $project '.tools/vpn-windows/'
$previous=Read-Stage $PreviousStage $PreviousInstaller $ExpectedPreviousInstallerSha256 $tools $project
$current=Read-Stage $Stage $Installer $ExpectedInstallerSha256 $tools $project
Assert-Upgrade $previous.version $current.version
foreach ($hive in @('HKCU','HKLM')) {Require (-not (Test-Path -LiteralPath ($hive+':\Software\Microsoft\Windows\CurrentVersion\Uninstall\TermTerm VPN Dev'))) 'Preserve an existing VPN development installation'}
$productKey='HKCU:\Software\termterm\TermTerm VPN Dev'
Require (-not (Test-Path -LiteralPath $productKey)) 'Preserve existing VPN development installer preferences'
Require (@(Get-Process -Name termterm-vpn-dev,termterm-vpn-broker,termterm-wireguard -ErrorAction SilentlyContinue).Count -eq 0) 'Wait for coordinated native quiet; preserve existing sessions'
$webviewKeys=@('HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}','HKLM:\Software\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}','HKLM:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}')
$webview=@($webviewKeys|ForEach-Object {if (Test-Path -LiteralPath $_) {(Get-ItemProperty -LiteralPath $_).pv}}|Where-Object {$_ -match '^\d+\.\d+\.\d+\.\d+$'})
Require ($webview.Count -gt 0) 'Existing WebView2 is required; this test must not install a shared host prerequisite'
$shortcuts=@((Join-Path ([Environment]::GetFolderPath('Desktop')) 'TermTerm VPN Dev.lnk'),(Join-Path ([Environment]::GetFolderPath('Programs')) 'TermTerm VPN Dev.lnk'),(Join-Path ([Environment]::GetFolderPath('Programs')) 'TermTerm VPN Dev/TermTerm VPN Dev.lnk'))
foreach ($path in $shortcuts) {Require (-not (Test-Path -LiteralPath $path)) 'Preserve an existing VPN development shortcut'}
$case=Join-Path $tools ('installer-lifecycle-'+[Guid]::NewGuid().ToString('N'));Assert-NewDirectory $case $tools
$data=@((Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'local.termterm.desktop.vpndev'),(Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'local.termterm.desktop.vpndev'))
foreach ($path in $data) {Assert-PlainPath $path;Require ([IO.Path]::GetPathRoot($path) -ieq [IO.Path]::GetPathRoot($case)) 'Data backup requires a same-volume directory rename'}
$stableBefore=Read-StableInstallation
$null=New-Item -ItemType Directory -Path $case
$acl=New-Object Security.AccessControl.DirectorySecurity;$acl.SetAccessRuleProtection($true,$false)
foreach ($sid in @([Security.Principal.WindowsIdentity]::GetCurrent().User,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))}
Set-Acl -LiteralPath $case -AclObject $acl
$moved=@{};$prepared=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase);$report=[ordered]@{ok=$false;previousVersion=$previous.version;version=$current.version;identity='local.termterm.desktop.vpndev';vaultOpened=$false;nativeVpnTested=$false;liveVpnUiCloseTested=$false;dataRestored=$false}
$target=Join-Path $case 'installed';$installed=$false;$phase='prepare-data'
try {
    Prepare-TestData $data $case $moved $prepared
    $index=0
    foreach ($package in @($previous,$current)) {
        $label=@('previous','current')[$index];$phase=$label+'-install'
        $installed=$true;Run-Setup $package.installer @('/S',('/D='+$target))
        Verify-Installed $target $package
        $report[$label+'InstalledPayloadVerified']=$true
        foreach ($path in $data) {Require ([IO.File]::ReadAllText((Join-Path $path 'installer-acceptance-sentinel')) -ceq 'preserve-disposable-development-data') 'Upgrade changed retained disposable data'}
        $phase=$label+'-window';$report[$label+'Launch']=Check-Window (Join-Path $target 'termterm-vpn-dev.exe') $case $label
        $index++
    }
    $report['installedPayloadVerified']=$true;$report['realWindowsAndGracefulClose']=$true;$report['upgradeRetainedData']=$true
    $phase='uninstall';Run-Setup (Join-Path $target 'uninstall.exe') @('/S',('_?='+$target));$installed=$false
    Require (-not (Test-Path -LiteralPath (Join-Path $target 'termterm-vpn-dev.exe')) -and -not (Test-Path -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TermTerm VPN Dev')) 'Uninstaller left the application or registration'
    foreach ($path in $shortcuts) {Require (-not (Test-Path -LiteralPath $path)) 'Uninstaller left an owned shortcut'}
    $report['uninstalled']=$true;$report.ok=$true
} catch {$report['failure']=$_.Exception.Message;$report['failedPhase']=$phase;if ($phase.StartsWith('previous-')) {$report['oldInstallFailure']=[ordered]@{phase=$phase;reasonCode=$_.Exception.Message;launchDiagnostic=(Join-Path $case 'previous-desktop.launch.json')}};throw}
finally {
    try {
        if ($installed -and (Test-Path -LiteralPath (Join-Path $target 'uninstall.exe'))) {Run-Setup (Join-Path $target 'uninstall.exe') @('/S',('_?='+$target))}
        if (Test-Path -LiteralPath $productKey) {$key=Get-Item -LiteralPath $productKey;Require ($key.GetValue('') -ceq $target -and @($key.GetSubKeyNames()).Count -eq 0) 'Unexpected installer preferences retained for review';Remove-Item -LiteralPath $productKey}
    } catch {$report.ok=$false;$report['cleanupFailure']=$_.Exception.Message}
    try {
        Require (@(Get-Process -Name termterm-vpn-dev,termterm-vpn-broker,termterm-wireguard -ErrorAction SilentlyContinue).Count -eq 0) 'Development process still running; original data backups retained'
        Restore-TestData $data $case $moved $prepared
        $report.dataRestored=$true
    } catch {$report.ok=$false;$report['dataRestoreFailure']=$_.Exception.Message}
    try {Require ((Read-StableInstallation) -ceq $stableBefore) 'Stable installation metadata changed during acceptance';$report['stableInstallationUnchanged']=$true} catch {$report.ok=$false;$report['stableInstallationUnchanged']=$false}
    [IO.File]::WriteAllText((Join-Path $case 'result.json'),($report|ConvertTo-Json -Depth 6))
}
Require $report.ok ('VPN development installer acceptance failed; preserve '+$case)
Write-Output ('PASS: isolated development install/window/upgrade/uninstall and original data restoration; '+$current.version)
