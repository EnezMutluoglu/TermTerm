param(
    [string]$Application,
    [string]$ExpectedManifestSha256,
    [string]$Fixtures,
    [string]$ExpectedFixturesSha256,
    [switch]$JournalAuditOnly,
    [switch]$SelfTest
)
$ErrorActionPreference='Stop'
$locks=[Collections.Generic.List[IO.FileStream]]::new()
$brokerReport=[ordered]@{ok=$false;scope='native Windows Team required-VPN SSH/SFTP';recoveryPerformed=$false;physicalPacketCaptureTested=$false}
$script:cleanupDeadline=$null
$script:cleanupCapReached=$false
function Require($Value,[string]$Message) {if (-not $Value) {throw $Message}}
function Lock-VerifiedFile([string]$Path,[string]$Hash) {
    Require ($Hash -cmatch '^[0-9a-f]{64}$') 'A reviewed SHA256 is required'
    $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    $locks.Add($stream)
    $hasher=[Security.Cryptography.SHA256]::Create()
    try {$actual=([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace('-','').ToLowerInvariant()} finally {$hasher.Dispose()}
    Require ($actual -ceq $Hash) ('Reviewed file changed: '+[IO.Path]::GetFileName($Path))
}
function Import-ValidatorFunctions([string]$Path,[string[]]$Names) {
    $errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile($Path,[ref]$null,[ref]$errors)
    Require (-not $errors) 'Reviewed validator does not parse'
    foreach ($name in $Names) {
        $definitions=@($ast.EndBlock.Statements | Where-Object {$_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -ceq $name})
        Require ($definitions.Count -eq 1) 'Reviewed validator function missing or duplicated'
        $body=$definitions[0].Body.Extent.Text
        $parameters=@($definitions[0].Parameters | ForEach-Object {$_.Extent.Text}) -join ','
        Set-Item -Path ('function:script:'+$name) -Value ([scriptblock]::Create(('param('+$parameters+")`n"+$body.Substring(1,$body.Length-2))))
    }
}
function Assert-Audit($Value,[string]$Phase,[string]$Hash) {
    Require ($Value.ok -is [bool] -and $Value.ok -and $Value.phase -ceq $Phase -and $Value.helperSha256 -ceq $Hash -and $Value.readOnly -is [bool] -and $Value.readOnly -and $Value.recoveryPerformed -is [bool] -and -not $Value.recoveryPerformed -and ($Value.protectedJournalCount -is [int] -or $Value.protectedJournalCount -is [long]) -and $Value.protectedJournalCount -eq 0) 'Read-only exact-helper zero-journal audit required'
}
function Assert-NativeResult($Value,[bool]$Shared) {
    foreach ($field in @('realTeamRequiredVpn','realSSH','vpnDisconnectClosedSSH','vpnDisconnectClosedSFTP','pausedTransferCancelled','existingTargetPreserved','preLogoutCleanupVerified','confirmedNativeCleanup')) {
        Require ($Value.$field -is [bool] -and $Value.$field) ('Native acceptance missing: '+$field)
    }
    Require ($Value.result -ceq 'passed' -and $Value.sftpSha256 -cmatch '^[0-9a-f]{64}$' -and $Value.publicVpnKeyLeaks -eq 0 -and $Value.localPartialFiles -eq 0 -and $Value.sharedRevocation -is [bool] -and $Value.sharedRevocation -eq $Shared) 'Native result identity or preservation proof differs'
    if ($Shared) {
        foreach ($field in @('recipientAclRevocation','ownerSharedAccessRetained','sharedCopyIndependent','recipientConnectOnly','revokedMaterialAndReconnectDenied','personalAuthenticationStorageIsolated')) {Require ($Value.$field -is [bool] -and $Value.$field) ('Native shared acceptance missing: '+$field)}
        Require ($Value.revocationShutdownSeconds -gt 0 -and $Value.revocationShutdownSeconds -le 15) 'Revocation did not close dependent sessions within fifteen seconds'
    }
}
function Wait-StrictCleanup([string]$Baseline,[int]$Seconds=720) {
    if (-not $script:cleanupDeadline) {$script:cleanupDeadline=[DateTime]::UtcNow.AddSeconds($Seconds)}
    do {
        if ([DateTime]::UtcNow -ge $script:cleanupDeadline) {$script:cleanupCapReached=$true;throw 'Full network baseline and strict native absence exceeded the test observation limit; no recovery performed'}
        if ((Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress) -ceq $Baseline) {
            try {return Read-NativeInventory 'after' 5000 500}
            catch {if ($_.Exception.Message -cne 'Native inventory did not prove no Wintun devices and no owned registered identities') {throw}}
        }
        Start-Sleep -Milliseconds 500
    } while ([DateTime]::UtcNow -lt $script:cleanupDeadline)
    $script:cleanupCapReached=$true
    throw 'Full network baseline and strict native absence exceeded the test observation limit; no recovery performed'
}
function Wait-File([string]$Path,[int]$Seconds) {
    $deadline=[DateTime]::UtcNow.AddSeconds($Seconds)
    while (-not (Test-Path -LiteralPath $Path)) {
        Require ([DateTime]::UtcNow -lt $deadline) ('Timed out waiting for '+[IO.Path]::GetFileName($Path))
        if ($script:native -and $script:native.HasExited) {throw 'Native example exited before the required external cleanup proof'}
        Start-Sleep -Milliseconds 200
    }
}
function Wait-TestProcessesExited {
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {
        if (@(Get-Process -Name 'termterm-vpn-broker','termterm-wireguard' -ErrorAction SilentlyContinue).Count -eq 0) {return}
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Native test broker or WireGuard process remained after owner exit'
}
if ($SelfTest) {
    $audit=[pscustomobject]@{ok=$true;phase='before';helperSha256=('a'*64);readOnly=$true;recoveryPerformed=$false;protectedJournalCount=[long]0}
    Assert-Audit $audit 'before' ('a'*64)
    $checks=1
    foreach ($field in @('ok','readOnly','recoveryPerformed','protectedJournalCount','helperSha256','phase')) {
        $changed=$audit.PSObject.Copy()
        $changed.$field=switch ($field) {'ok' {$false} 'readOnly' {$false} 'recoveryPerformed' {$true} 'protectedJournalCount' {[long]1} 'helperSha256' {('b'*64)} 'phase' {'after-normal'}}
        $failed=$false;try {Assert-Audit $changed 'before' ('a'*64)} catch {$failed=$true}
        Require $failed 'Unsafe audit receipt was accepted';$checks++
    }
    $sample=[pscustomobject]@{result='passed';realTeamRequiredVpn=$true;realSSH=$true;sftpSha256=('a'*64);vpnDisconnectClosedSSH=$true;vpnDisconnectClosedSFTP=$true;pausedTransferCancelled=$true;existingTargetPreserved=$true;preLogoutCleanupVerified=$true;confirmedNativeCleanup=$true;publicVpnKeyLeaks=0;localPartialFiles=0;sharedRevocation=$true;recipientAclRevocation=$true;ownerSharedAccessRetained=$true;sharedCopyIndependent=$true;recipientConnectOnly=$true;revokedMaterialAndReconnectDenied=$true;personalAuthenticationStorageIsolated=$true;revocationShutdownSeconds=3.5}
    Assert-NativeResult $sample $true;$checks++
    foreach ($field in @('existingTargetPreserved','preLogoutCleanupVerified','revokedMaterialAndReconnectDenied','personalAuthenticationStorageIsolated','revocationShutdownSeconds')) {
        $changed=$sample.PSObject.Copy();$changed.$field=if ($field -ceq 'revocationShutdownSeconds') {15.1} else {$false}
        $failed=$false;try {Assert-NativeResult $changed $true} catch {$failed=$true}
        Require $failed 'Incomplete native result was accepted';$checks++
    }
    $temporary=Join-Path ([IO.Path]::GetTempPath()) ('termterm-team-vpn-pin-'+[Guid]::NewGuid().ToString('N'))
    try {
        [IO.File]::WriteAllText($temporary,'disposable hash fixture')
        $hash=(Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant()
        Lock-VerifiedFile $temporary $hash;$checks++
        $failed=$false;try {Lock-VerifiedFile $temporary ('0'*64)} catch {$failed=$true}
        Require $failed 'Changed acceptance member passed its reviewed hash';$checks++
    } finally {foreach ($lock in $locks) {$lock.Dispose()};Remove-Item -LiteralPath $temporary}
    $script:cleanupDeadline=[DateTime]::UtcNow.AddSeconds(-1)
    $failed=$false;try {Wait-StrictCleanup 'unused'} catch {$failed=$true}
    Require ($failed -and $script:cleanupCapReached -and $script:cleanupDeadline -lt [DateTime]::UtcNow) 'Expired cleanup observation started another wait';$checks++
    [ordered]@{selfTest=$true;passed=$checks;networkActions=$false;privilegedActions=$false} | ConvertTo-Json -Compress
    return
}
Require ($Application -and $ExpectedManifestSha256 -and $Fixtures -and $ExpectedFixturesSha256) 'Reviewed application, acceptance manifest and fixture hashes required'
$applicationPath=(Resolve-Path -LiteralPath $Application).Path
$bundle=Split-Path -Parent $applicationPath
$worktree=[IO.DirectoryInfo]$bundle
while ($worktree -and $worktree.Name -cne '.tools') {$worktree=$worktree.Parent}
Require ($worktree -and [IO.Path]::GetFileName($applicationPath) -ceq 'termterm.exe') 'Use a staged test example under this worktree .tools'
$project=$worktree.Parent.FullName
Require ($bundle.StartsWith((Join-Path $project '.tools/vpn-windows/'),[StringComparison]::OrdinalIgnoreCase)) 'Use an isolated VPN test bundle'
$fixturePath=(Resolve-Path -LiteralPath $Fixtures).Path
$fixtureDirectory=Split-Path -Parent $fixturePath
Require ($fixturePath.StartsWith((Join-Path $project '.lab/'),[StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($fixturePath) -ceq 'fixture.json') 'Disposable fixture must stay in this worktree .lab'
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
Require ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) -eq [bool]$JournalAuditOnly) 'Native example must be unelevated; the separate read-only audit must be elevated'
$native=$null;$baseline=$null;$verified=$false
try {
    $manifestPath=Join-Path $bundle 'team-vpn-acceptance.json';Lock-VerifiedFile $manifestPath $ExpectedManifestSha256
    $manifest=Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    Require ($manifest.version -eq 1 -and $manifest.developmentOnly -is [bool] -and $manifest.developmentOnly -and $manifest.fixtureId -cmatch '^[0-9a-f]{16}$') 'Matching development acceptance manifest required'
    $required=@('termterm.exe','termterm-vpn-broker.exe','vpn-dev-bundle.json','team_vpn_windows.ps1','validate-full-broker-v5.ps1','validate-dns-v1.ps1','validate-dns-journal-audit-v1.ps1')
    foreach ($name in $required) {Require ($name -cin @($manifest.files.name)) 'Required reviewed bundle member missing'}
    foreach ($file in $manifest.files) {
        Require ($file.name -cmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$') 'Invalid acceptance manifest filename'
        Lock-VerifiedFile (Join-Path $bundle $file.name) $file.sha256
    }
    Lock-VerifiedFile $PSCommandPath ($manifest.files | Where-Object name -CEQ 'team_vpn_windows.ps1').sha256
    Lock-VerifiedFile $fixturePath $ExpectedFixturesSha256
    $fixture=Get-Content -LiteralPath $fixturePath -Raw | ConvertFrom-Json
    Require ($fixture.version -eq 1 -and $fixture.fixtureId -ceq $manifest.fixtureId -and [IO.Path]::GetFullPath($fixture.root) -ceq $fixtureDirectory -and $fixture.vpn.family -ceq 'wireGuard' -and $fixture.vpn.config.Contains('Endpoint = 127.0.0.1:31820') -and $fixture.vpn.config.Contains('AllowedIPs = 10.97.0.1/32') -and $fixture.sharedRevocation -is [bool]) 'Frozen disposable Team/WireGuard fixture identity changed'
    Require (([uri]$fixture.profile.serviceUrl).Scheme -ceq 'https' -and ([uri]$fixture.profile.serviceUrl).Host -ceq 'localhost') 'Disposable Team HTTPS must use localhost'
    $helperPath=Join-Path $bundle 'termterm-vpn-broker.exe'
    $helperHash=($manifest.files | Where-Object name -CEQ 'termterm-vpn-broker.exe').sha256
    Import-ValidatorFunctions (Join-Path $bundle 'validate-full-broker-v5.ps1') @('Assert-NativeInventory')
    Import-ValidatorFunctions (Join-Path $bundle 'validate-dns-v1.ps1') @('Wait-InventoryOutput','Invoke-BoundedInventoryProbe','Read-NativeInventory','Read-DnsBaseline')
    $verified=$true
    $auditScript=Join-Path $project 'vpn-windows-broker/scripts/validate-dns-journal-audit-v1.ps1'
    Lock-VerifiedFile $auditScript ($manifest.files | Where-Object name -CEQ 'validate-dns-journal-audit-v1.ps1').sha256
    if ($JournalAuditOnly) {
        $null=Read-NativeInventory 'before'
        & $auditScript -Helper $helperPath -ExpectedHelperSha256 $helperHash -Phase before
        [IO.File]::WriteAllText((Join-Path $fixtureDirectory 'audit-ready'),'read-only-before-audit-complete')
        Wait-File (Join-Path $fixtureDirectory 'client/audit-request-ready') 1250
        $null=Read-NativeInventory 'after'
        & $auditScript -Helper $helperPath -ExpectedHelperSha256 $helperHash -Phase after-normal
        return
    }
    $reportPath=Join-Path $bundle 'team-vpn-windows-result.json'
    Require (-not (Test-Path -LiteralPath $reportPath)) 'Preserve previous native report; use a fresh bundle'
    Wait-File (Join-Path $fixtureDirectory 'audit-ready') 120
    $beforeAudit=Get-Content -LiteralPath (Join-Path $bundle 'dns-journal-audit-before.json') -Raw | ConvertFrom-Json
    Assert-Audit $beforeAudit 'before' $helperHash
    Require (@(Get-Process -Name 'termterm-vpn-broker','termterm-wireguard' -ErrorAction SilentlyContinue).Count -eq 0) 'Preserve an existing native VPN process; wait for coordinated quiet'
    foreach ($name in @('revocation-cleanup-ready','revocation-cleanup-verified','audit-request-ready','native-result.json','download','preserved')) {Require (-not (Test-Path -LiteralPath (Join-Path $fixtureDirectory ('client/'+$name)))) 'Preserve previous native fixture outputs; generate a fresh fixture'}
    $brokerReport['beforeNativeInventory']=Read-NativeInventory 'before'
    $baseline=Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress
    Require (-not (Test-Path -LiteralPath (Join-Path $fixtureDirectory 'windows-network-baseline.json'))) 'Preserve previous private network baseline'
    [IO.File]::WriteAllText((Join-Path $fixtureDirectory 'windows-network-baseline.json'),$baseline)
    $relay=Get-Content -LiteralPath (Join-Path $fixtureDirectory 'relay-receipt.json') -Raw | ConvertFrom-Json
    Require ($relay.fixtureId -ceq $fixture.fixtureId -and $relay.loopbackOnly -is [bool] -and $relay.loopbackOnly -and $relay.stopped -is [bool] -and -not $relay.stopped) 'Fresh loopback-only ciphertext relay required'
    $probe=[Net.Sockets.TcpClient]::new()
    try {
        $connect=$probe.ConnectAsync('10.97.0.1',41843)
        $reachable=$false;try {$reachable=$connect.Wait(1000) -and $probe.Connected} catch {}
        Require (-not $reachable) 'SSH target reachable without required VPN'
    } finally {$probe.Dispose()}
    $brokerReport['targetUnreachableWithoutVpn']=$true
    $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$applicationPath;$start.WorkingDirectory=$fixtureDirectory
    $start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    $start.EnvironmentVariables['TERMTERM_TEAM_VPN_NATIVE_FIXTURE']=$fixturePath
    $native=[Diagnostics.Process]::new();$native.StartInfo=$start
    Require ($native.Start()) 'Native Team example did not start'
    $stdout=$native.StandardOutput.ReadToEndAsync();$stderr=$native.StandardError.ReadToEndAsync()
    Wait-File (Join-Path $fixtureDirectory 'client/revocation-cleanup-ready') 1050
    $brokerReport['preLogoutNativeInventory']=Wait-StrictCleanup $baseline
    [IO.File]::WriteAllText((Join-Path $fixtureDirectory 'client/audit-request-ready'),'terminal-disconnected-and-strict-native-baseline-restored')
    Wait-File (Join-Path $bundle 'dns-journal-audit-after-normal.json') 120
    $afterAudit=Get-Content -LiteralPath (Join-Path $bundle 'dns-journal-audit-after-normal.json') -Raw | ConvertFrom-Json
    Assert-Audit $afterAudit 'after-normal' $helperHash
    [IO.File]::WriteAllText((Join-Path $fixtureDirectory 'client/revocation-cleanup-verified'),'zero-protected-journals-and-strict-native-baseline-before-logout')
    Require ($native.WaitForExit(60000) -and $native.ExitCode -eq 0) 'Native Team example did not finish successfully; preserve private native logs'
    $value=Get-Content -LiteralPath (Join-Path $fixtureDirectory 'client/native-result.json') -Raw | ConvertFrom-Json
    Assert-NativeResult $value $fixture.sharedRevocation
    Require ($value.sftpSha256 -ceq (Get-FileHash -LiteralPath (Join-Path $fixtureDirectory 'client/payload') -Algorithm SHA256).Hash.ToLowerInvariant()) 'Native transfer hash differs from the disposable payload'
    $brokerReport['native']=$value;$brokerReport['beforeJournalAudit']=$beforeAudit;$brokerReport['preLogoutJournalAudit']=$afterAudit
    $brokerReport['afterNativeInventory']=Wait-StrictCleanup $baseline
    Wait-TestProcessesExited
    $brokerReport['nativeBrokerAndEngineProcesses']=0
    $brokerReport['fullNetworkBaselineRestored']=$true;$brokerReport['preLogoutCleanupVerified']=$true;$brokerReport['ok']=$true
} catch {
    $brokerReport['failureType']=$_.Exception.GetType().FullName
    throw
} finally {
    if (-not $JournalAuditOnly) {
        if ($native) {
            if (-not $native.HasExited) {$native.Kill();$null=$native.WaitForExit(5000)}
            if ($stdout -and $stdout.Wait(2000)) {[IO.File]::WriteAllText((Join-Path $fixtureDirectory 'native-stdout.log'),$stdout.GetAwaiter().GetResult())}
            if ($stderr -and $stderr.Wait(2000)) {[IO.File]::WriteAllText((Join-Path $fixtureDirectory 'native-stderr.log'),$stderr.GetAwaiter().GetResult())}
            $native.Dispose();$native=$null
        }
        if ($verified -and $baseline) {
            try {
                if ($script:cleanupCapReached) {
                    $brokerReport['observationCapReached']=$true
                    Require ((Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress) -ceq $baseline) 'Final network baseline still differs'
                    $brokerReport['finalNativeInventory']=Read-NativeInventory 'final' 1000 500
                } else {$brokerReport['finalNativeInventory']=Wait-StrictCleanup $baseline}
                $brokerReport['finalFullNetworkBaselineRestored']=$true
                [IO.File]::WriteAllText((Join-Path $fixtureDirectory 'client/audit-request-ready'),'strict-native-baseline-restored')
            }
            catch {$brokerReport['finalFullNetworkBaselineRestored']=$false;$brokerReport['ok']=$false}
        }
        if ($reportPath) {[IO.File]::WriteAllText($reportPath,($brokerReport | ConvertTo-Json -Depth 15))}
    }
    foreach ($lock in $locks) {$lock.Dispose()}
}
Require $brokerReport.ok 'Native Windows Team acceptance failed; retain private fixture and redacted report'
