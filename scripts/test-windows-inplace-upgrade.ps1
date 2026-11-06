param(
    [ValidateSet('Plan','Prepare','Install','Verify')][string]$Action='Plan',
    [string]$Case,
    [string]$ReuseBackupCase,
    [switch]$SelfTest
)
# Exact local 0.4.2 payload upgrade only; GUI smoke needs a separate OS account because startup can open the remembered vault.
$ErrorActionPreference='Stop'
if ($PSVersionTable.PSVersion -lt [version]'7.5') {throw 'Use PowerShell 7.5+'}
$project=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$target='C:\TermTerm'
$caseRoot=Join-Path $project '.tools/windows-inplace-upgrade/cases'
$packageRoot=Join-Path $project '.tools/vpn-windows/package-20261105-candidate-v1'
$pins=@{installer='6c52b9d233b1c1c989d05123880459090b0371fe3bff68fdb1986f850b6b2f00';manifest='1bdce8309f5a4da453e5e9677bebaa55fcb44d37baa55c36e648620e20a9099b';desktop='3c407c45d51f52e944bc51cf4be6d8e08a61831eb6788c34951f7ded74e5a0e0';nsis='ed91b827ae7331527431ef06dc3773fa6089bb11a8fe6f9b64a2750631cbfc4a'}
$oldDesktopSha='b4d8abbd12ccbb72c0879c5e587bbe31eba3a236e48d9e633bb71d13527b5844'
$data=@((Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'local.termterm.desktop'),(Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'local.termterm.desktop'))
# Import only path/hash helpers; the development lifecycle runner moves data and must never execute here.
$tokens=$null;$parseErrors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'test-vpn-windows-installer.ps1'),[ref]$tokens,[ref]$parseErrors)
if ($parseErrors.Count) {throw 'Path helper source does not parse'}
$helpers=@($ast.EndBlock.Statements|Where-Object {$_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -cin @('Require','Assert-PlainPath','Assert-NewDirectory','File-Hash')})
if ($helpers.Count -ne 4) {throw 'Path helper set differs'}
. ([scriptblock]::Create(($helpers.Extent.Text -join "`n")))
function Json($Value) {ConvertTo-Json -InputObject $Value -Depth 20 -Compress}
function Snapshot([string]$Root) {
    Assert-PlainPath $Root
    $pending=[Collections.Generic.Stack[string]]::new();$pending.Push($Root);$rows=[Collections.Generic.List[object]]::new()
    while($pending.Count) {
        $directory=$pending.Pop()
        foreach($entry in @((Get-Item -LiteralPath $directory -Force))+@(Get-ChildItem -LiteralPath $directory -Force)) {
            Require (-not ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'Linked backup sources are refused'
            if($entry.PSIsContainer -and $entry.FullName -cne $directory){$pending.Push($entry.FullName);continue}
            $rows.Add([ordered]@{relative=[IO.Path]::GetRelativePath($Root,$entry.FullName);directory=[bool]$entry.PSIsContainer;length=$(if($entry.PSIsContainer){0}else{$entry.Length});sha256=$(if($entry.PSIsContainer){$null}else{File-Hash $entry.FullName});acl=(Get-Acl -LiteralPath $entry.FullName).Sddl})
        }
    }
    return @($rows|Sort-Object {$_.relative})
}
function Content($Rows) {Json @($Rows|ForEach-Object {[ordered]@{relative=$_.relative;directory=$_.directory;length=$_.length;sha256=$_.sha256}})}
function Backup([string]$Source,[string]$Name) {
    $before=@(Snapshot $Source);$backup=Join-Path $Case $Name;$reused=$false
    if($ReuseBackupCase) {
        $previous=Join-Path $ReuseBackupCase $Name
        if((Test-Path -LiteralPath $previous -PathType Container) -and (Content $before) -ceq (Content @(Snapshot $previous))) {$backup=$previous;$reused=$true}
    }
    if(-not $reused) {
        Assert-NewDirectory $backup $Case;$null=New-Item -ItemType Directory -Path $backup
        foreach($row in $before) {if($row.relative -ceq '.'){continue};$out=Join-Path $backup $row.relative;if($row.directory){$null=New-Item -ItemType Directory -Path $out -Force}else{$null=New-Item -ItemType Directory -Path (Split-Path $out -Parent) -Force;[IO.File]::Copy((Join-Path $Source $row.relative),$out,$false)}}
    }
    Require ((Content $before) -ceq (Content @(Snapshot $backup))) 'Backup content differs'
    Require ((Json $before) -ceq (Json @(Snapshot $Source))) 'Source content/ACL changed during backup'
    return [ordered]@{path=$Source;backup=$backup;reused=$reused;rows=$before}
}
function Quiet {
    Require (@(Get-Process -Name termterm,termterm-vpn-broker,termterm-openvpn,termterm-openconnect,termterm-wireguard -ErrorAction SilentlyContinue).Count -eq 0) 'TermTerm or a helper is running; no process is terminated'
    Require (@(Get-CimInstance -Query "SELECT CommandLine FROM Win32_Process WHERE Name='msedgewebview2.exe'"|Where-Object {$_.CommandLine -match 'local\.termterm\.desktop(?:[\\/\s"]|$)'}).Count -eq 0) 'Production WebView is running; leave live data in place'
}
function Assert-NoRegistration {
    foreach($root in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall','HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall','HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall')) {
        foreach($key in @(Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)) {$value=Get-ItemProperty -LiteralPath $key.PSPath;Require ($value.DisplayName -ine 'TermTerm' -and $key.PSChildName -notin @('TermTerm','local.termterm.desktop')) 'This case covers the observed unregistered 0.4.2 payload only; registered/MSI upgrades require separate review'}
    }
}
function Registry {
    return @(foreach($hive in @('HKCU','HKLM')) {foreach($suffix in @('Software\termterm\TermTerm','Software\Microsoft\Windows\CurrentVersion\Uninstall\TermTerm','Software\Microsoft\Windows\CurrentVersion\Uninstall\local.termterm.desktop','Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\TermTerm')) {
        $path=$hive+':\'+$suffix;if(Test-Path -LiteralPath $path){$key=Get-Item -LiteralPath $path;$values=@(foreach($name in @($key.GetValueNames()|Sort-Object)){[ordered]@{name=$name;kind=[string]$key.GetValueKind($name);value=$key.GetValue($name)}});[ordered]@{path=$path;values=$values}}
    }})
}
function Shortcuts {
    $shell=New-Object -ComObject WScript.Shell
    $rows=@(foreach($root in @('Desktop','Programs','CommonDesktopDirectory','CommonPrograms')|ForEach-Object {[Environment]::GetFolderPath($_)}|Select-Object -Unique) {
        if(-not $root -or -not(Test-Path -LiteralPath $root)){continue}
        foreach($file in @(Get-ChildItem -LiteralPath $root -Filter '*.lnk' -File -Recurse)) {$link=$shell.CreateShortcut($file.FullName);if($link.TargetPath -ieq (Join-Path $target 'termterm.exe')){[ordered]@{path=$file.FullName;target=$link.TargetPath;arguments=$link.Arguments;sha256=(File-Hash $file.FullName);acl=(Get-Acl -LiteralPath $file.FullName).Sddl}}}
    })
    return @($rows|Sort-Object {$_.path})
}
function Vault-Files([string]$Vault) {
    return @(foreach($suffix in @('','-journal','-wal','-shm','.lock')) {$path=$Vault+$suffix;Assert-PlainPath $path;$exists=Test-Path -LiteralPath $path -PathType Leaf;[ordered]@{path=$path;exists=[bool]$exists;sha256=$(if($exists){File-Hash $path}else{$null});acl=$(if($exists){(Get-Acl -LiteralPath $path).Sddl}else{$null})}})
}
function Read-AutoStart([string]$Path='HKCU:\Software\Microsoft\Windows\CurrentVersion\Run') {
    if(Test-Path -LiteralPath $Path){(Get-Item -LiteralPath $Path).GetValue('TermTerm',$null)}
}
function Assert-UpdateScript([string]$Text) {
    Require ($Text -match '(?s)Function PageLeaveReinstall\s+.*?\$\{If\} \$UpdateMode = 1\s+Goto reinst_done\s+\$\{EndIf\}' -and $Text -match '(?s)Function \.onInstSuccess\s+.*?\$\{GetOptions\} \$CMDLINE "/R".*?\$\{IfNot\} \$\{Errors\}.*?RunAsUser') 'UPDATE must skip uninstall and silent launch must require /R'
    $install=[regex]::Match($Text,'(?ms)^Section Install\s+(.*?)^SectionEnd').Groups[1].Value
    Require ($install -and $install -notmatch '\$(?:LOCAL)?APPDATA') 'Install section must not mutate production user data'
}
function Package {
    $installer=Join-Path $packageRoot 'TermTerm_0.4.3_x64-setup.exe';$stage=Join-Path $packageRoot 'installer-desktop';$nsis=Join-Path $packageRoot 'target/release/nsis/x64/installer.nsi'
    foreach($pair in @(@($installer,$pins.installer),@((Join-Path $stage 'vpn-bundle.json'),$pins.manifest),@($nsis,$pins.nsis))) {Assert-PlainPath $pair[0];Require ((File-Hash $pair[0]) -ceq $pair[1]) 'Exact reviewed candidate package differs'}
    Assert-UpdateScript (Get-Content -LiteralPath $nsis -Raw)
    $manifest=Get-Content -LiteralPath (Join-Path $stage 'vpn-bundle.json') -Raw|ConvertFrom-Json
    Require ($manifest.version -eq 1 -and $manifest.applicationIdentifier -ceq 'local.termterm.desktop' -and $manifest.candidateVersion -ceq '0.4.3' -and $manifest.developmentOnly -is [bool] -and -not $manifest.developmentOnly -and $manifest.files.'termterm.exe' -ceq $pins.desktop) 'Exact production 0.4.3 NSIS manifest required'
    foreach($file in $manifest.files.PSObject.Properties){Require ($file.Name -cmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$' -and $file.Value -cmatch '^[0-9a-f]{64}$') 'Unsafe bundle member';Assert-PlainPath (Join-Path $stage $file.Name);Require ((File-Hash (Join-Path $stage $file.Name)) -ceq $file.Value) 'Staged payload differs'}
    return @{installer=$installer;manifest=$manifest}
}
function Save {[IO.File]::WriteAllText((Join-Path $Case 'upgrade-state.json'),(Json $script:state))}
function Assert-Preserved {
    foreach($snapshot in $state.appData){Require ((Json $snapshot.rows) -ceq (Json @(Snapshot $snapshot.path))) 'Live AppData content/ACL changed';Require ((Content $snapshot.rows) -ceq (Content @(Snapshot $snapshot.backup))) 'AppData backup changed'}
    Require ((Json $state.vaultFiles) -ceq (Json @(Vault-Files $state.vault))) 'External vault/sidecar content, existence or ACL changed'
    foreach($row in $state.vaultCopies){Require ((File-Hash $row.copy) -ceq $row.sha256) 'Vault backup changed'}
    Require ((Json $state.shortcuts) -ceq (Json @(Shortcuts))) 'Original shortcut inventory/content/ACL changed'
    Require ((Read-AutoStart) -ceq $state.autoStart) 'Autostart changed'
}
function Verify {
    Quiet;Assert-Preserved
    foreach($file in $package.manifest.files.PSObject.Properties){Require ((File-Hash (Join-Path $target $file.Name)) -ceq $file.Value) 'Installed companion payload differs'}
    Require ((File-Hash (Join-Path $target 'vpn-bundle.json')) -ceq $pins.manifest -and (Get-Item -LiteralPath (Join-Path $target 'termterm.exe')).VersionInfo.ProductVersion -ceq '0.4.3') 'Installed version/manifest differs'
    $registration=Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\TermTerm'
    Require ($registration.DisplayVersion -ceq '0.4.3' -and $registration.InstallLocation.Trim('"') -ceq $target -and $registration.MainBinaryName -ceq 'termterm.exe' -and $registration.UninstallString.Trim('"') -ceq (Join-Path $target 'uninstall.exe')) 'Installed registry identity/version/location differs'
    Require ((Get-Item -LiteralPath 'HKCU:\Software\termterm\TermTerm').GetValue('') -ceq $target) 'Installer preference points elsewhere'
    $state.registryAfter=@(Registry);$state.phase='verified';$state.verifiedUtc=[DateTime]::UtcNow.ToString('o');Save
    [ordered]@{case=$Case;ok=$true;payloadUpgrade='0.4.2 -> 0.4.3';previousRegisteredInstallation=$false;newRegistrationVerified=$true;userDataUnchanged=$true;shortcutsUnchanged=$true;uninstallPerformed=$false;applicationLaunched=$false;nativeVpnTested=$false}|ConvertTo-Json -Compress
}
if($SelfTest) {
    $fixture='Function PageLeaveReinstall'+"`n"+'${If} $UpdateMode = 1'+"`nGoto reinst_done`n"+'${EndIf}'+"`nFunctionEnd`nSection Install`nFile payload`nSectionEnd`nFunction .onInstSuccess`n"+'${GetOptions} $CMDLINE "/R" $R0'+"`n"+'${IfNot} ${Errors}'+"`nRunAsUser`nFunctionEnd"
    Assert-UpdateScript $fixture;$checks=1
    foreach($bad in @($fixture.Replace('Goto reinst_done','Goto reinst_uninstall'),$fixture.Replace('"/R"','"/OTHER"'),$fixture.Replace('File payload','Delete "$APPDATA\data"'))) {$failed=$false;try{Assert-UpdateScript $bad}catch{$failed=$true};Require $failed 'Unsafe installer accepted';$checks++}
    $test=Join-Path ([IO.Path]::GetTempPath()) ('termterm-inplace-selftest-'+[Guid]::NewGuid().ToString('N'));$null=New-Item -ItemType Directory -Path $test
    try {
        $Case=Join-Path $test 'case';$null=New-Item -ItemType Directory -Path $Case;$source=Join-Path $test 'source';$null=New-Item -ItemType Directory -Path $source;[IO.File]::WriteAllText((Join-Path $source 'fixture'),'keep')
        $original=@(Snapshot $source);$copy=Backup $source 'copy';Require (-not $copy.reused -and (Json $original) -ceq (Json @(Snapshot $source))) 'Backup moved/changed live fixture';$checks++
        $ReuseBackupCase=$Case;$Case=Join-Path $test 'next';$null=New-Item -ItemType Directory -Path $Case;$copy=Backup $source 'copy';Require ($copy.reused -and -not(Test-Path -LiteralPath (Join-Path $Case 'copy'))) 'Identical backup not reused';$checks++
        [IO.File]::WriteAllText((Join-Path $source 'fixture'),'changed');$copy=Backup $source 'copy';Require (-not $copy.reused -and [IO.File]::ReadAllText((Join-Path $ReuseBackupCase 'copy/fixture')) -ceq 'keep') 'Stale backup reused or overwritten';$checks++
        $failed=$false;try{$null=Backup $source 'copy'}catch{$failed=$true};Require $failed 'Backup collision overwritten';$checks++
        $vault=Join-Path $test 'fixture.ttvault';[IO.File]::WriteAllText($vault,'encrypted fixture');$before=@(Vault-Files $vault);[IO.File]::WriteAllText(($vault+'-wal'),'new sidecar');Require ((Json $before) -cne (Json @(Vault-Files $vault))) 'New sidecar missed';$checks++
        $key='HKCU:\Software\TermTermInplaceSelfTest-'+[Guid]::NewGuid().ToString('N')
        try {Require ($null -eq (Read-AutoStart $key)) 'Absent registry key rejected';$null=New-Item -Path $key;Require ($null -eq (Read-AutoStart $key)) 'Absent optional value rejected';$null=New-ItemProperty -LiteralPath $key -Name TermTerm -Value 'inert fixture';Require ((Read-AutoStart $key) -ceq 'inert fixture') 'Present optional value lost';$checks++} finally {if(Test-Path -LiteralPath $key){Remove-Item -LiteralPath $key}}
    } finally {$full=[IO.Path]::GetFullPath($test);Require ([IO.Path]::GetDirectoryName($full) -ceq [IO.Path]::GetTempPath().TrimEnd('\') -and [IO.Path]::GetFileName($full) -cmatch '^termterm-inplace-selftest-[0-9a-f]{32}$') 'Unsafe fixture cleanup';Remove-Item -LiteralPath $full -Recurse -Force}
    [ordered]@{selfTest=$true;passed=$checks;installerExecuted=$false;userDataMoved=$false}|ConvertTo-Json -Compress;return
}
Assert-PlainPath $target
if($Action -ceq 'Plan') {$null=Package;Quiet;[ordered]@{target=$target;installerArguments=@('/S','/UPDATE','/D=C:\TermTerm');packagePinsVerified=$true;appDataKeptInPlace=$true;applicationLaunch=$false;registeredUpgradeCoverage=$false;mutationPerformed=$false}|ConvertTo-Json -Compress;return}
Require (-not ([Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) 'Use the unelevated current-user installer'
if($Action -ceq 'Prepare' -and -not $Case){$Case=Join-Path $caseRoot ('upgrade-'+[Guid]::NewGuid().ToString('N'))}
Require (-not [string]::IsNullOrWhiteSpace($Case)) 'Use the case returned by Prepare'
$Case=[IO.Path]::GetFullPath($Case);Assert-PlainPath $Case
Require ([IO.Path]::GetDirectoryName($Case) -ceq $caseRoot -and [IO.Path]::GetFileName($Case) -cmatch '^upgrade-[0-9a-f]{32}$') 'Use a fresh owned upgrade case'
$package=Package;Quiet
if($Action -ceq 'Prepare') {
    Assert-NoRegistration
    Require ((File-Hash (Join-Path $target 'termterm.exe')) -ceq $oldDesktopSha -and (Get-Item -LiteralPath (Join-Path $target 'termterm.exe')).VersionInfo.ProductVersion -ceq '0.4.2') 'Observed old 0.4.2 payload changed'
    if($ReuseBackupCase){$ReuseBackupCase=[IO.Path]::GetFullPath($ReuseBackupCase);Assert-PlainPath $ReuseBackupCase;Require ($ReuseBackupCase.StartsWith((Join-Path $project '.tools')+'\',[StringComparison]::OrdinalIgnoreCase)) 'Reuse only private worktree backups'}
    Assert-NewDirectory $Case $caseRoot;$null=New-Item -ItemType Directory -Path $Case -Force
    $acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetAccessRuleProtection($true,$false)
    foreach($sid in @([Security.Principal.WindowsIdentity]::GetCurrent().User,[Security.Principal.SecurityIdentifier]::new('S-1-5-18'))){$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))};Set-Acl -LiteralPath $Case -AclObject $acl
    $script:state=[ordered]@{version=1;case=$Case;target=$target;phase='backing-up';createdUtc=[DateTime]::UtcNow.ToString('o');pins=$pins;registryBefore=@(Registry);shortcuts=@(Shortcuts);autoStart=(Read-AutoStart);installerExecuted=$false;userDataMoved=$false};Save
    $state.install=Backup $target 'installed-app-backup';$state.appData=@(for($i=0;$i -lt $data.Count;$i++){Backup $data[$i] ('appdata-copy-'+$i)})
    $recent=Get-Content -LiteralPath (Join-Path $data[0] 'recent.json') -Raw|ConvertFrom-Json;$state.vault=[IO.Path]::GetFullPath([string]$recent.path);Require ([IO.Path]::GetExtension($state.vault) -ieq '.ttvault' -and (Test-Path -LiteralPath $state.vault -PathType Leaf)) 'Actual recent encrypted vault required'
    $state.vaultFiles=@(Vault-Files $state.vault);$state.vaultCopies=@();$null=New-Item -ItemType Directory -Path (Join-Path $Case 'vault-copy')
    foreach($row in $state.vaultFiles){if(-not $row.exists){continue};$copy=Join-Path $Case ('vault-copy/vault.ttvault'+$row.path.Substring($state.vault.Length));[IO.File]::Copy($row.path,$copy,$false);Require ((File-Hash $copy) -ceq $row.sha256) 'Vault backup differs';$state.vaultCopies+=,[ordered]@{copy=$copy;sha256=$row.sha256}}
    for($i=0;$i -lt $state.shortcuts.Count;$i++){[IO.File]::Copy($state.shortcuts[$i].path,(Join-Path $Case ('shortcut-'+$i+'.lnk')),$false)}
    Quiet;Assert-Preserved;$state.phase='prepared';Save
    [ordered]@{case=$Case;prepared=$true;installBackupReused=$state.install.reused;appDataBackupsReused=@($state.appData|ForEach-Object {$_.reused});userDataMoved=$false;installerExecuted=$false}|ConvertTo-Json -Compress;return
}
$script:state=Get-Content -LiteralPath (Join-Path $Case 'upgrade-state.json') -Raw|ConvertFrom-Json -DateKind String -AsHashtable
Require ($state.version -eq 1 -and $state.case -ceq $Case -and $state.target -ceq $target -and $state.userDataMoved -is [bool] -and -not $state.userDataMoved) 'Owned upgrade state differs'
foreach($name in $pins.Keys){Require ($state.pins[$name] -ceq $pins[$name]) 'Prepared package pin changed'}
if($Action -ceq 'Verify'){Require ($state.installerExited -and $state.installerExitCode -eq 0) 'Successful owned installer exit required';Verify;return}
Require ($state.phase -ceq 'prepared' -and -not $state.installerExecuted) 'Never repeat an installer phase'
Assert-NoRegistration;Assert-Preserved
Require ((Json $state.install.rows) -ceq (Json @(Snapshot $target)) -and (Content $state.install.rows) -ceq (Content @(Snapshot $state.install.backup))) 'Old installation/backup changed'
Require ((Json $state.registryBefore) -ceq (Json @(Registry))) 'Installation registry changed after backup'
$state.phase='installing';$state.installerExecuted=$true;$state.installerExited=$false;Save
$process=$null
try {
    $process=Start-Process -FilePath $package.installer -ArgumentList @('/S','/UPDATE','/D=C:\TermTerm') -WindowStyle Hidden -PassThru;$null=$process.Handle
    $state.installerPid=$process.Id;$state.installerBirthUtc=$process.StartTime.ToUniversalTime().ToString('o');Save
    $deadline=[DateTime]::UtcNow.AddMinutes(3)
    while(-not $process.WaitForExit(1000)){Require ([DateTime]::UtcNow -lt $deadline) 'Installer still running; no kill, rollback or data restoration is attempted'}
    $state.installerExited=$true;$state.installerExitCode=$process.ExitCode;Save;Require ($process.ExitCode -eq 0) 'Installer failed; retain every backup and installed state'
    Verify
} catch {$state.phase='failed';$state.failure=$_.Exception.Message;Save;throw} finally {if($process){$process.Dispose()}}
