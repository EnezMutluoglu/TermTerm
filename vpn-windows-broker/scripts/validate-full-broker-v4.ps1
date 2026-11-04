param(
    [string]$Helper,
    [string]$ExpectedManifestSha256,
    [string]$Fixtures,
    [string]$ExpectedFixturesSha256,
    [switch]$SelfTest
)
$ErrorActionPreference='Stop'
$locks=[Collections.Generic.List[IO.FileStream]]::new()
$brokerProcess=$null
$bundleVerified=$false
$brokerReport=[ordered]@{ok=$false;scenario='two OpenVPN plus OpenConnect';driverMayRemainInstalled=$true;cleanupEvidence='pending-authenticated-receipt';events=@();pings=@()}
$script:brokerReplies=@{}
$script:brokerStates=@{}
$script:brokerSessions=@{}
$script:brokerAllowedFailures=@{}
$script:brokerRequestId=0
$script:brokerReadTask=$null

function Read-LockedFile([string]$Path,[string]$ExpectedHash) {
    $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    $locks.Add($stream)
    $hasher=[Security.Cryptography.SHA256]::Create()
    try {$hash=([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace('-','').ToLowerInvariant()} finally {$hasher.Dispose()}
    if ($hash -ne $ExpectedHash.ToLowerInvariant()) {throw "Approved file hash changed: $([IO.Path]::GetFileName($Path))"}
    $stream.Position=0
    $reader=[IO.StreamReader]::new($stream,[Text.Encoding]::UTF8,$true,4096,$true)
    try {$reader.ReadToEnd()} finally {$reader.Dispose()}
}
function Read-NetworkBaseline {
    [ordered]@{
        addresses=@(Get-NetIPAddress | Select-Object InterfaceIndex,AddressFamily,IPAddress,PrefixLength,PrefixOrigin,SuffixOrigin | Sort-Object InterfaceIndex,AddressFamily,IPAddress)
        routes=@(Get-NetRoute | Select-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop,RouteMetric,Protocol | Sort-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop)
        interfaces=@(Get-NetIPInterface | Select-Object InterfaceIndex,AddressFamily,NlMtu,Dhcp,RouterDiscovery,Forwarding | Sort-Object InterfaceIndex,AddressFamily)
        dns=@(Get-DnsClientServerAddress | Select-Object InterfaceIndex,AddressFamily,@{Name='Servers';Expression={$_.ServerAddresses -join ','}} | Sort-Object InterfaceIndex,AddressFamily)
    }
}
function Assert-NativeInventory($Value,[int]$ExitCode) {
    if ($null -eq $Value -or $ExitCode -ne 0 -or $Value.readOnly -isnot [bool] -or -not $Value.readOnly -or $Value.safe -isnot [bool] -or -not $Value.safe -or $Value.devices -isnot [Array] -or $Value.registeredTermTermInterfaces -isnot [Array] -or $Value.devices.Count -ne 0 -or $Value.registeredTermTermInterfaces.Count -ne 0) {throw 'Native inventory did not prove no Wintun devices and no owned registered identities'}
}
function Read-NativeInventory([string]$Phase) {
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$helperPath;$start.Arguments='--wintun-sweep-preflight';$start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    $probe=[Diagnostics.Process]::new();$probe.StartInfo=$start
    try {
        if (-not $probe.Start()) {throw 'Native inventory process did not start'}
        $stdout=$probe.StandardOutput.ReadToEndAsync();$stderr=$probe.StandardError.ReadToEndAsync()
        if (-not $probe.WaitForExit(10000)) {$probe.Kill();$probe.WaitForExit();throw 'Read-only native inventory timed out'}
        $text=$stdout.GetAwaiter().GetResult();$errorText=$stderr.GetAwaiter().GetResult()
        $brokerReport[$Phase+'NativeExitCode']=$probe.ExitCode;$brokerReport[$Phase+'NativeStderr']=$errorText
        $value=$text | ConvertFrom-Json
        Assert-NativeInventory $value $probe.ExitCode
        return $value
    } finally {$probe.Dispose()}
}
function Send-BrokerRequest([hashtable]$Value,[bool]$AllowFailure=$false) {
    $script:brokerRequestId++
    $Value['requestId']=$script:brokerRequestId
    if ($AllowFailure) {$script:brokerAllowedFailures["$script:brokerRequestId"]=$true}
    $line=$Value | ConvertTo-Json -Depth 24 -Compress
    if ([Text.Encoding]::UTF8.GetByteCount($line) -ge 2097152) {throw 'Control frame exceeds limit'}
    $brokerProcess.StandardInput.WriteLine($line)
    $brokerProcess.StandardInput.Flush()
    $script:brokerRequestId
}
function Resolve-FixtureAnswer($Field,$Credentials) {
    if (@($Field.choices).Count) {throw 'Disposable fixture requested an unexpected authentication field'}
    if ($Field.name -eq 'username' -and $Field.secret -is [bool] -and -not $Field.secret -and $Field.purpose -in @('text','username')) {$value=$Credentials.username}
    elseif ($Field.name -eq 'password' -and $Field.secret -is [bool] -and $Field.secret -and $Field.purpose -in @('secret','password')) {$value=$Credentials.password}
    else {throw 'Disposable fixture requested an unexpected authentication field'}
    if ($value -isnot [string] -or -not $value.Length -or $value.Length -gt 4096 -or $value.Contains([char]0)) {throw 'Disposable fixture credential missing or invalid'}
    return $value
}
function Receive-BrokerMessage([UInt64]$CleanupRequestId=0) {
    if ([DateTime]::UtcNow -gt $brokerDeadline) {throw 'Broker test deadline exceeded'}
    if (-not $script:brokerReadTask) {$script:brokerReadTask=$brokerProcess.StandardOutput.ReadLineAsync()}
    while (-not $script:brokerReadTask.Wait(200)) {
        if ([DateTime]::UtcNow -gt $brokerDeadline) {throw 'Broker test deadline exceeded'}
    }
    $line=$script:brokerReadTask.GetAwaiter().GetResult()
    $script:brokerReadTask=$null
    if ($null -eq $line) {throw 'Broker closed before expected state'}
    if ([Text.Encoding]::UTF8.GetByteCount($line) -gt 2097152) {throw 'Broker response exceeds limit'}
    $message=$line | ConvertFrom-Json
    if ($message.kind -eq 'reply') {
        $script:brokerReplies["$($message.requestId)"]=$message
        if (-not $message.ok -and -not $script:brokerAllowedFailures.ContainsKey("$($message.requestId)")) {
            if ($CleanupRequestId -and [UInt64]$message.requestId -ne $CleanupRequestId) {
                $brokerReport['cleanupPriorFailures']+=@([ordered]@{kind='reply';requestId=$message.requestId;error=$message.error})
                $brokerReport['ok']=$false
            } else {throw "Broker rejected request: $($message.error)"}
        }
    } elseif ($message.kind -eq 'session') {
        $session=$message.session
        if (-not $script:brokerSessions.ContainsKey($session.id)) {throw 'Unexpected session identity'}
        if ([UInt64]$session.generation -ne [UInt64]$script:brokerSessions[$session.id].generation) {return}
        $script:brokerStates[$session.id]=$session.state
        $brokerReport.events+=@([ordered]@{id=$session.id;generation=$session.generation;state=$session.state})
        if ($session.state -eq 'failed') {
            if ($CleanupRequestId) {
                $brokerReport['cleanupPriorFailures']+=@([ordered]@{kind='session';id=$session.id;generation=$session.generation;error=$session.error})
                $brokerReport['ok']=$false
            } else {throw "VPN session failed: $($session.error)"}
        }
        if ($session.state -eq 'challenge' -and -not $CleanupRequestId) {
            $values=@{}
            foreach ($field in $session.challenge.fields) {
                # Saved profile passwords are intentionally empty; only the locked fixture supplies test answers.
                $values[$field.name]=Resolve-FixtureAnswer $field $fixtureData
            }
            $null=Send-BrokerRequest @{action='answer';id=$session.id;generation=$session.generation;challengeId=$session.challenge.id;values=$values}
        }
    } else {throw 'Unexpected broker frame kind'}
}
function Wait-BrokerReply([UInt64]$RequestId,[switch]$Cleanup) {
    while (-not $script:brokerReplies.ContainsKey("$RequestId")) {
        if ($Cleanup) {Receive-BrokerMessage $RequestId} else {Receive-BrokerMessage}
    }
    $script:brokerReplies["$RequestId"]
}
function Confirm-BrokerCleanup($Receipt) {
    if ($Receipt.kind -ne 'reply' -or $Receipt.ok -isnot [bool] -or -not $Receipt.ok -or $Receipt.result.cleanupVerified -isnot [bool] -or -not $Receipt.result.cleanupVerified -or ($Receipt.result.remainingJournals -isnot [int] -and $Receipt.result.remainingJournals -isnot [long]) -or ($Receipt.result.remainingSessions -isnot [int] -and $Receipt.result.remainingSessions -isnot [long]) -or $Receipt.result.remainingJournals -ne 0 -or $Receipt.result.remainingSessions -ne 0) {throw 'Authenticated broker did not verify global protected-journal cleanup'}
    $brokerReport['cleanupEvidence']='verified-by-authenticated-broker'
    $brokerReport['remainingProtectedJournals']=$Receipt.result.remainingJournals
    $brokerReport['remainingSessions']=$Receipt.result.remainingSessions
}
function Wait-AllConnected {
    while (@($script:brokerSessions.Keys | Where-Object {$script:brokerStates[$_] -ne 'connected'}).Count) {Receive-BrokerMessage}
}
function Test-PacketTraffic([string[]]$Ids,[string]$Phase) {
    $payload=[Text.Encoding]::ASCII.GetBytes('termterm-fixed-vpn-payload-v1')
    foreach ($id in $Ids) {
        $target=$script:brokerSessions[$id].fixture.target
        if ($target -notin @('10.88.0.1','10.88.0.2','10.88.0.3')) {throw 'Unexpected packet fixture target'}
        $ping=[Net.NetworkInformation.Ping]::new()
        try {
            for ($i=0;$i -lt 3;$i++) {
                $reply=$ping.Send($target,2000,$payload)
                if ($reply.Status -ne [Net.NetworkInformation.IPStatus]::Success -or $reply.Address.ToString() -ne $target -or [Convert]::ToBase64String($reply.Buffer) -ne [Convert]::ToBase64String($payload)) {throw "ICMP payload check failed for fixture $target"}
                $brokerReport.pings+=@([ordered]@{phase=$Phase;target=$target;bytes=$reply.Buffer.Length;payloadVerified=$true})
            }
        } finally {$ping.Dispose()}
    }
}

if ($SelfTest) {
    $tests=@()
    function Reset-TestMessages([object[]]$Messages,[bool]$InitialOk=$false) {
        $script:brokerReplies=@{};$script:brokerStates=@{};$script:brokerSessions=@{};$script:brokerAllowedFailures=@{};$script:brokerReadTask=$null
        $script:brokerDeadline=[DateTime]::UtcNow.AddSeconds(2)
        $script:brokerReport=[ordered]@{ok=$InitialOk;error='original-connect-failure';events=@();cleanupEvidence='pending-authenticated-receipt';cleanupPriorFailures=@()}
        $lines=@($Messages | ForEach-Object {$_ | ConvertTo-Json -Depth 12 -Compress}) -join "`n"
        $script:brokerProcess=[pscustomobject]@{StandardOutput=[IO.StringReader]::new($lines)}
    }
    function Assert-Test([bool]$Condition,[string]$Message) {if (-not $Condition) {throw $Message}}
    $prior=@{kind='reply';requestId=10;ok=$false;error='delayed-connect-failure'}
    $close=@{kind='reply';requestId=12;ok=$true;result=@{cleanupVerified=$true;remainingJournals=0;remainingSessions=0}}
    foreach ($initialOk in @($false,$true)) {
        Reset-TestMessages @($prior,@{kind='reply';requestId=11;ok=$true},$close) $initialOk
        Confirm-BrokerCleanup (Wait-BrokerReply 12 -Cleanup)
        Assert-Test ($brokerReport.cleanupEvidence -eq 'verified-by-authenticated-broker' -and $brokerReport.cleanupPriorFailures.Count -eq 1 -and -not $brokerReport.ok -and $brokerReport.error -eq 'original-connect-failure') 'Cleanup hid a prior failure or lost the exact close receipt'
        $brokerProcess.StandardOutput.Dispose()
        $tests+=@("delayed-failure-close-success-initial-ok-$initialOk")
    }
    Reset-TestMessages @($prior,@{kind='reply';requestId=12;ok=$false;error='close-failure'})
    $failure='';try {$null=Wait-BrokerReply 12 -Cleanup} catch {$failure=$_.ToString()}
    Assert-Test ($failure -like '*close-failure*' -and $brokerReport.cleanupPriorFailures.Count -eq 1 -and -not $brokerReport.ok -and $brokerReport.error -eq 'original-connect-failure' -and $brokerReport.cleanupEvidence -ne 'verified-by-authenticated-broker') 'Failed close was accepted or original failure overwritten'
    $brokerProcess.StandardOutput.Dispose();$tests+=@('delayed-failure-close-failure')
    Reset-TestMessages @($prior)
    Receive-BrokerMessage 12
    $script:brokerReadTask=[Threading.Tasks.TaskCompletionSource[string]]::new().Task
    $script:brokerDeadline=[DateTime]::UtcNow.AddMilliseconds(-1)
    $failure='';try {$null=Wait-BrokerReply 12 -Cleanup} catch {$failure=$_.ToString()}
    Assert-Test ($failure -like '*deadline*' -and $brokerReport.cleanupPriorFailures.Count -eq 1 -and -not $brokerReport.ok -and $brokerReport.error -eq 'original-connect-failure' -and $brokerReport.cleanupEvidence -ne 'verified-by-authenticated-broker') 'Cleanup deadline was bypassed or failure overwritten'
    $brokerProcess.StandardOutput.Dispose();$script:brokerReadTask=$null;$tests+=@('delayed-failure-close-timeout')
    $session=@{id='fixture-session';generation=1;state='failed';error='delayed-session-failure'}
    $challenge=@{id='fixture-session';generation=1;state='challenge';challenge=@{fields=@(@{name='password'})}}
    Reset-TestMessages @(@{kind='session';session=$session},@{kind='session';session=$challenge},$close)
    $script:brokerSessions['fixture-session']=@{generation=1}
    Confirm-BrokerCleanup (Wait-BrokerReply 12 -Cleanup)
    Assert-Test ($brokerReport.cleanupPriorFailures.Count -eq 1 -and -not $brokerReport.ok -and $brokerReport.cleanupEvidence -eq 'verified-by-authenticated-broker') 'Cleanup did not drain session failures or attempted challenge answers'
    $brokerProcess.StandardOutput.Dispose();$tests+=@('session-failure-challenge-close-success')
    Reset-TestMessages @(@{kind='reply';requestId=12;ok=$true;result=@{cleanupVerified='false';remainingJournals=0;remainingSessions=0}})
    $failure='';try {Confirm-BrokerCleanup (Wait-BrokerReply 12 -Cleanup)} catch {$failure=$_.ToString()}
    Assert-Test ($failure -like '*did not verify*' -and -not $brokerReport.ok -and $brokerReport.cleanupEvidence -ne 'verified-by-authenticated-broker') 'Malformed cleanup receipt was accepted'
    $brokerProcess.StandardOutput.Dispose();$tests+=@('truthy-string-cleanup-rejected')
    $credentials=@{username='fixture-test-user';password='fixture-test-password';profile=@{username='wrong-profile-user';password=''}}
    $passwordField=@{name='password';secret=$true;purpose='secret';choices=@()}
    $usernameField=@{name='username';secret=$false;purpose='text';choices=@()}
    Assert-Test ((Resolve-FixtureAnswer $passwordField $credentials) -eq $credentials.password -and (Resolve-FixtureAnswer $usernameField $credentials) -eq $credentials.username) 'Challenge answers did not use the locked top-level fixture credentials'
    $tests+=@('blank-profile-uses-top-level-fixture-credentials')
    $failure='';try {$null=Resolve-FixtureAnswer $passwordField @{username='fixture-test-user';password='';profile=@{password='must-not-fallback'}}} catch {$failure=$_.ToString()}
    Assert-Test ($failure -like '*credential missing*') 'Missing fixture credential silently fell back to profile data'
    $tests+=@('missing-top-level-credential-rejected')
    $failure='';try {$null=Resolve-FixtureAnswer @{name='password';secret=$true;purpose='otp';choices=@()} $credentials} catch {$failure=$_.ToString()}
    Assert-Test ($failure -like '*unexpected authentication field*') 'Fixture password was offered to an OTP challenge'
    $tests+=@('otp-field-rejected')
    $failure='';try {$null=Resolve-FixtureAnswer @{name='other';secret=$true;purpose='secret';choices=@()} $credentials} catch {$failure=$_.ToString()}
    Assert-Test ($failure -like '*unexpected authentication field*') 'Unknown challenge was answered'
    $tests+=@('unknown-field-rejected')
    [ordered]@{selfTest=$true;passed=$tests.Count;tests=$tests;privilegedActions=$false;networkActions=$false} | ConvertTo-Json -Depth 4
    return
}
if (-not $Helper -or -not $ExpectedManifestSha256 -or -not $Fixtures -or -not $ExpectedFixturesSha256) {throw 'Helper, expected manifest hash, fixtures and expected fixtures hash are required'}
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {throw 'The VPN manager test must run unelevated; only its fixed helper requests UAC'}
$helperPath=(Resolve-Path -LiteralPath $Helper).Path
if ([IO.Path]::GetFileName($helperPath) -ne 'termterm-vpn-broker.exe') {throw 'Unexpected helper filename'}
$bundle=Split-Path -Parent $helperPath

try {
    if (@(Get-Process -Name 'termterm-vpn-broker' -ErrorAction SilentlyContinue).Count) {throw 'Another development broker is still active'}
    $manifestPath=Join-Path $bundle 'manifest.json'
    $manifest=(Read-LockedFile $manifestPath $ExpectedManifestSha256) | ConvertFrom-Json
    foreach ($required in @('termterm-vpn-broker.exe','termterm-openvpn.exe','termterm-openconnect.exe','wintun.dll','fixture-ca.pem')) {
        if ($required -notin @($manifest.files | ForEach-Object {$_.name})) {throw "Required bundle member missing: $required"}
    }
    foreach ($file in $manifest.files) {
        if ([IO.Path]::GetFileName($file.name) -ne $file.name) {throw 'Invalid manifest filename'}
        $null=Read-LockedFile (Join-Path $bundle $file.name) $file.sha256
    }
    $fixtureData=(Read-LockedFile (Resolve-Path -LiteralPath $Fixtures).Path $ExpectedFixturesSha256) | ConvertFrom-Json
    $fixturesToUse=@($fixtureData.probeProfiles)
    if ($fixturesToUse.Count -ne 3 -or @($fixturesToUse | Where-Object {$_.profile.family -eq 'openVpn'}).Count -ne 2 -or @($fixturesToUse | Where-Object {$_.profile.family -eq 'cisco'}).Count -ne 1) {throw 'Expected two OpenVPN and one OpenConnect fixture'}
    $brokerReport['manifestSha256']=$ExpectedManifestSha256
    $brokerReport['fixturesSha256']=$ExpectedFixturesSha256
    $bundleVerified=$true
    $brokerReport['beforeNativeInventory']=Read-NativeInventory 'before'
    $baseline=Read-NetworkBaseline
    $baselineJson=$baseline | ConvertTo-Json -Depth 8 -Compress
    [IO.File]::WriteAllText((Join-Path $bundle 'full-broker-baseline.json'),$baselineJson)
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$helperPath;$start.Arguments='--validation-client';$start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $start.RedirectStandardInput=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    $brokerProcess=[Diagnostics.Process]::new();$brokerProcess.StartInfo=$start
    if (-not $brokerProcess.Start()) {throw 'Validation client did not start'}
    $stderrTask=$brokerProcess.StandardError.ReadToEndAsync()
    $brokerDeadline=[DateTime]::UtcNow.AddSeconds(150)
    $hello=Send-BrokerRequest @{action='hello';vaultId=[Guid]::NewGuid().ToString()}
    $null=Wait-BrokerReply $hello
    $ids=@()
    foreach ($fixture in $fixturesToUse) {
        $id=[Guid]::NewGuid().ToString();$ids+=@($id)
        $script:brokerSessions[$id]=@{fixture=$fixture;generation=1}
        $null=Send-BrokerRequest @{action='connect';id=$id;generation=1;profileId=[Guid]::NewGuid().ToString();profile=$fixture.profile}
    }
    Wait-AllConnected
    Test-PacketTraffic $ids 'three-connected'
    $stop=Send-BrokerRequest @{action='disconnect';id=$ids[1];generation=1}
    $null=Wait-BrokerReply $stop
    Test-PacketTraffic @($ids[0],$ids[2]) 'middle-stopped'
    $script:brokerSessions[$ids[1]].generation=2;$script:brokerStates[$ids[1]]='connecting'
    $restart=Send-BrokerRequest @{action='connect';id=$ids[1];generation=2;profileId=[Guid]::NewGuid().ToString();profile=$fixturesToUse[1].profile}
    $null=Wait-BrokerReply $restart
    $stale=Send-BrokerRequest @{action='disconnect';id=$ids[1];generation=1} $true
    $staleReply=Wait-BrokerReply $stale
    if ($staleReply.ok -or $staleReply.error -notlike '*stale_session_generation*') {throw 'A stale disconnect was not rejected'}
    $brokerReport['staleGenerationRejected']=$true
    Wait-AllConnected
    Test-PacketTraffic $ids 'reconnected'
    $brokerReport['connectedFamilies']=@('openVpn','openVpn','cisco')
    $brokerReport['realVpnAuthenticationTested']=$true
    $brokerReport['kernelPacketExchangeTested']=$true
    $brokerReport['trafficPassed']=$true
    $brokerReport['ok']=$true
} catch {$brokerReport['error']=$_.ToString();throw}
finally {
    if ($brokerProcess) {
        try {
            if (-not $brokerProcess.HasExited) {
                try {
                    $brokerDeadline=[DateTime]::UtcNow.AddSeconds(20)
                    $close=Send-BrokerRequest @{action='close'}
                    $brokerReport['cleanupPriorFailures']=@()
                    $receipt=Wait-BrokerReply $close -Cleanup
                    Confirm-BrokerCleanup $receipt
                } catch {$brokerReport['cleanupError']=$_.ToString();$brokerReport['ok']=$false}
                finally {$brokerProcess.StandardInput.Close()}
            }
            if (-not $brokerProcess.WaitForExit(10000)) {$brokerProcess.Kill();$brokerProcess.WaitForExit();$brokerReport['ok']=$false;$brokerReport['clientForcedToExit']=$true}
            $brokerReport['clientExitCode']=$brokerProcess.ExitCode
            if ($brokerProcess.ExitCode -ne 0) {$brokerReport['ok']=$false}
            if ($stderrTask) {$brokerReport['clientStderr']=$stderrTask.GetAwaiter().GetResult()}
        } catch {$brokerReport['cleanupError']=$_.ToString();$brokerReport['ok']=$false}
        finally {$brokerProcess.Dispose()}
    }
    try {
        $waitUntil=[DateTime]::UtcNow.AddSeconds(20)
        while (@(Get-Process -Name 'termterm-vpn-broker' -ErrorAction SilentlyContinue).Count -and [DateTime]::UtcNow -lt $waitUntil) {Start-Sleep -Milliseconds 200}
        $brokerReport['remainingBrokerPids']=@(Get-Process -Name 'termterm-vpn-broker' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
        $brokerReport['remainingTermTermAdapters']=@(Get-NetAdapter -IncludeHidden | Where-Object {$_.Name -like 'TermTerm VPN *'} | Select-Object Name,InterfaceGuid,InterfaceIndex)
        if ($baselineJson) {$brokerReport['addressRouteInterfaceDnsBaselineRestored']=((Read-NetworkBaseline | ConvertTo-Json -Depth 8 -Compress) -eq $baselineJson)}
        if ($bundleVerified) {$brokerReport['afterNativeInventory']=Read-NativeInventory 'after'}
        if (@($brokerReport.remainingBrokerPids).Count -or @($brokerReport.remainingTermTermAdapters).Count -or -not $brokerReport.addressRouteInterfaceDnsBaselineRestored -or $brokerReport.cleanupEvidence -ne 'verified-by-authenticated-broker') {$brokerReport['ok']=$false}
    } catch {$brokerReport['finalDiagnosticError']=$_.ToString();$brokerReport['ok']=$false}
    try {[IO.File]::WriteAllText((Join-Path $bundle 'full-broker-result.json'),($brokerReport | ConvertTo-Json -Depth 12))}
    finally {foreach ($stream in $locks) {$stream.Dispose()}}
}
if (-not $brokerReport.ok) {throw 'Full broker validation failed; see the durable result report'}
