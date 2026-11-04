param(
    [Parameter(Mandatory=$true)][string]$Helper,
    [Parameter(Mandatory=$true)][string]$ExpectedManifestSha256,
    [Parameter(Mandatory=$true)][string]$Fixtures,
    [Parameter(Mandatory=$true)][string]$ExpectedFixturesSha256
)
$ErrorActionPreference='Stop'
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {throw 'The VPN manager test must run unelevated; only its fixed helper requests UAC'}
$helperPath=(Resolve-Path -LiteralPath $Helper).Path
if ([IO.Path]::GetFileName($helperPath) -ne 'termterm-vpn-broker.exe') {throw 'Unexpected helper filename'}
$bundle=Split-Path -Parent $helperPath
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
function Receive-BrokerMessage {
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
        if (-not $message.ok -and -not $script:brokerAllowedFailures.ContainsKey("$($message.requestId)")) {throw "Broker rejected request: $($message.error)"}
    } elseif ($message.kind -eq 'session') {
        $session=$message.session
        if (-not $script:brokerSessions.ContainsKey($session.id)) {throw 'Unexpected session identity'}
        if ([UInt64]$session.generation -ne [UInt64]$script:brokerSessions[$session.id].generation) {return}
        $script:brokerStates[$session.id]=$session.state
        $brokerReport.events+=@([ordered]@{id=$session.id;generation=$session.generation;state=$session.state})
        if ($session.state -eq 'failed') {throw "VPN session failed: $($session.error)"}
        if ($session.state -eq 'challenge') {
            $profile=$script:brokerSessions[$session.id].fixture.profile
            $values=@{}
            foreach ($field in $session.challenge.fields) {
                if ($field.name -eq 'username') {$values[$field.name]=$profile.username}
                elseif ($field.name -eq 'password') {$values[$field.name]=$profile.password}
                else {throw 'Disposable fixture requested an unexpected authentication field'}
            }
            $null=Send-BrokerRequest @{action='answer';id=$session.id;generation=$session.generation;challengeId=$session.challenge.id;values=$values}
        }
    } else {throw 'Unexpected broker frame kind'}
}
function Wait-BrokerReply([UInt64]$RequestId) {
    while (-not $script:brokerReplies.ContainsKey("$RequestId")) {Receive-BrokerMessage}
    $script:brokerReplies["$RequestId"]
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
                    $receipt=Wait-BrokerReply $close
                    if (-not $receipt.ok -or -not $receipt.result.cleanupVerified -or $receipt.result.remainingJournals -ne 0 -or $receipt.result.remainingSessions -ne 0) {throw 'Authenticated broker did not verify global protected-journal cleanup'}
                    $brokerReport['cleanupEvidence']='verified-by-authenticated-broker'
                    $brokerReport['remainingProtectedJournals']=$receipt.result.remainingJournals
                    $brokerReport['remainingSessions']=$receipt.result.remainingSessions
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
