param(
    [string]$Helper,
    [string]$ExpectedManifestSha256,
    [string]$Fixtures,
    [string]$ExpectedFixturesSha256,
    [ValidateSet('normal','client-death','wrong-key','silent-loss')][string]$Mode='normal',
    [switch]$SelfTest
)
$ErrorActionPreference='Stop'
$locks=[Collections.Generic.List[IO.FileStream]]::new()
function Require($Value,[string]$Message) {if (-not $Value) {throw $Message}}
function Import-ValidatorFunctions([string]$File,[string[]]$Names) {
    $bytes=[IO.File]::ReadAllBytes((Join-Path $PSScriptRoot $File));$errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseInput([Text.Encoding]::UTF8.GetString($bytes),$File,[ref]$null,[ref]$errors)
    Require (-not $errors) 'Reviewed validator does not parse'
    foreach ($name in $Names) {
        $definitions=@($ast.EndBlock.Statements | Where-Object {$_ -is [Management.Automation.Language.FunctionDefinitionAst] -and $_.Name -ceq $name})
        Require ($definitions.Count -eq 1) 'Reviewed validator function missing or duplicated'
        $body=$definitions[0].Body.Extent.Text
        $parameters=@($definitions[0].Parameters | ForEach-Object {$_.Extent.Text}) -join ','
        Set-Item -Path ('function:script:'+$name) -Value ([scriptblock]::Create(('param('+$parameters+")`n"+$body.Substring(1,$body.Length-2))))
    }
}
Import-ValidatorFunctions 'validate-full-broker-v5.ps1' @('Read-LockedFile','Assert-NativeInventory','Send-BrokerRequest','Resolve-FixtureAnswer','Receive-BrokerMessage','Wait-BrokerReply','Confirm-BrokerCleanup','Wait-AllConnected')
Import-ValidatorFunctions 'validate-dns-v1.ps1' @('Read-SharedJson','Test-JsonInteger','Wait-InventoryOutput','Invoke-BoundedInventoryProbe','Read-NativeInventory','Read-DnsBaseline','Wait-DnsBaseline','Wait-NativeCleanup','Assert-OwnedDns','Read-DnsCounts','Invoke-ResolverCheck','Test-NormalResolver','Start-DnsClient')
$script:sendBrokerInner=(Get-Item function:Send-BrokerRequest).ScriptBlock
$script:receiveBrokerInner=(Get-Item function:Receive-BrokerMessage).ScriptBlock
$script:nativeInventoryInner=(Get-Item function:Read-NativeInventory).ScriptBlock
$script:pendingError='cleanup_pending: windows_interface_unregister_pending'
$script:observationCapSeconds=720
function Send-BrokerRequest([hashtable]$Value,[bool]$AllowFailure=$false) {
    if ($script:activeCase -and $Value.action -ceq 'quiesced' -and $Value.id -ceq $script:activeCase.id) {$script:activeCase['quiesceAckSentMs']=$script:caseClock.ElapsedMilliseconds}
    $setupRequest=$script:activeSetup -and $Value.id -ceq $script:activeSetup.id -and $Value.action -in @('connect','networkReceipt')
    if ($setupRequest) {Set-SetupStage ('sending-'+$Value.action);$script:activeSetup['expectedRequestId']=$script:brokerRequestId+1}
    $requestId=& $script:sendBrokerInner $Value $AllowFailure
    if ($script:traceSetupRequests) {
        Require (-not $script:setupRequestActions.ContainsKey("$requestId") -and $script:setupRequestActions.Count -lt 128) 'Setup trace request ID reused or trace limit exceeded'
        $script:setupRequestActions["$requestId"]=[ordered]@{action=$Value.action;id=$Value.id;generation=$Value.generation}
    }
    if ($setupRequest) {
        $script:activeSetup.requests+=@([ordered]@{action=$Value.action;requestId=$requestId;elapsedMs=$script:setupClock.ElapsedMilliseconds})
        $script:activeSetup[$Value.action+'RequestId']=$requestId;Set-SetupStage ('waiting-'+$Value.action+'-reply')
    }
    $requestId
}
function Receive-BrokerMessage([uint64]$CleanupRequestId=0) {
    $message=$null;$readerWasPresent=$null -ne $script:brokerReadTask
    try {. $script:receiveBrokerInner $CleanupRequestId}
    finally {
        if ($script:activeSetup) {
            $script:activeSetup['readCalls']=$script:activeSetup.readCalls+1
            $script:activeSetup['lastReaderState']=[ordered]@{wasPresent=$readerWasPresent;stillPresent=($null -ne $script:brokerReadTask);isCompleted=if ($script:brokerReadTask) {$script:brokerReadTask.IsCompleted} else {$null};elapsedMs=$script:setupClock.ElapsedMilliseconds}
            if ($message.kind -ceq 'reply') {
                $known=$script:traceSetupRequests -and $script:setupRequestActions.ContainsKey("$($message.requestId)")
                $action=if ($known) {$script:setupRequestActions["$($message.requestId)"].action} else {$null}
                Require ($script:activeSetup.replies.Count -lt 128) 'Setup reply trace limit exceeded'
                $script:activeSetup.replies+=@([ordered]@{requestId=$message.requestId;action=$action;knownRequest=$known;ok=$message.ok;elapsedMs=$script:setupClock.ElapsedMilliseconds})
                if ($message.requestId -eq $script:activeSetup.connectRequestId) {$script:activeSetup['connectReplyMs']=$script:setupClock.ElapsedMilliseconds;Set-SetupStage 'waiting-connected-event'}
                if ($message.requestId -eq $script:activeSetup.networkReceiptRequestId) {$script:activeSetup['networkReceiptReplyMs']=$script:setupClock.ElapsedMilliseconds;Set-SetupStage 'verifying-owned-network'}
            } elseif ($message.kind -ceq 'session') {
                $current=$message.session.id -ceq $script:activeSetup.id -and $message.session.generation -eq 1
                Require ($script:activeSetup.events.Count -lt 128) 'Setup event trace limit exceeded'
                $script:activeSetup.events+=@([ordered]@{id=$message.session.id;generation=$message.session.generation;state=$message.session.state;currentSetup=$current;elapsedMs=$script:setupClock.ElapsedMilliseconds})
                if ($current -and $message.session.state -ceq 'connected') {$script:activeSetup['connectedMs']=$script:setupClock.ElapsedMilliseconds;Set-SetupStage 'connected'}
            }
        }
        if ($script:activeCase) {
            $state=$script:brokerStates[$script:activeCase.id]
            if ($state -cne $script:activeCase.lastObservedState) {
                $elapsed=$script:caseClock.ElapsedMilliseconds;$script:activeCase['lastObservedState']=$state
                $script:activeCase.events+=@([ordered]@{state=$state;elapsedMs=$elapsed})
                if ($state -ceq 'disconnecting') {$script:activeCase['disconnectingMs']=$elapsed}
                if ($state -in @('failed','disconnected')) {$script:activeCase['terminalMs']=$elapsed}
            }
            if ($message.kind -ceq 'session' -and $message.session.id -ceq $script:activeCase.id -and $message.session.state -ceq 'disconnecting' -and $message.session.error -ceq $script:pendingError -and -not $script:activeCase.Contains('pendingObservedMs')) {
                $script:activeCase['pendingObservedMs']=$script:caseClock.ElapsedMilliseconds
                $script:activeCase['pendingPublicError']=$script:pendingError
                Test-PendingActorResponsive $script:activeCase.id
            }
        }
    }
}
function Set-SetupStage([string]$Stage) {
    if ($script:activeSetup) {
        Require ($script:activeSetup.stages.Count -lt 128) 'Setup stage trace limit exceeded'
        $script:activeSetup['stage']=$Stage
        $script:activeSetup.stages+=@([ordered]@{stage=$Stage;elapsedMs=$script:setupClock.ElapsedMilliseconds;ownerHasExited=[bool]$brokerProcess.HasExited})
    }
}
function Start-SetupAttempt([string]$Id,[string]$Kind) {
    $script:setupClock=[Diagnostics.Stopwatch]::StartNew()
    $script:activeSetup=[ordered]@{id=$Id;generation=1;kind=$Kind;startedUtc=[DateTime]::UtcNow.ToString('o');status='pending';stage='before-connect-send';requests=@();replies=@();events=@();stages=@();readCalls=0;ownerPid=$brokerProcess.Id;requestCounterBefore=$script:brokerRequestId;readerPresentBefore=($null -ne $script:brokerReadTask)}
    $brokerReport.setupAttempts+=@($script:activeSetup);Set-SetupStage 'before-connect-send'
}
function Complete-SetupAttempt {
    Set-SetupStage 'complete';$script:activeSetup['status']='passed';$script:activeSetup=$null
}
function Start-ObservationCase([string]$Id,[string]$Kind) {
    $script:caseClock=[Diagnostics.Stopwatch]::StartNew()
    $script:activeCase=[ordered]@{kind=$Kind;id=$Id;startedUtc=[DateTime]::UtcNow.ToString('o');status='pending';events=@();nativeSamples=@();lastObservedState='connected';observationCapSeconds=$script:observationCapSeconds;observationDeadlineMs=($script:observationCapSeconds*1000);observationLimit='test observation limit; no product cleanup cutoff'}
    $script:activeCase
}
function Assert-PendingActorReplies($Sessions,$Close,[string]$Id,[long]$SessionsMs,[long]$CloseMs) {
    Require ($SessionsMs -lt 6000 -and $CloseMs -lt 6000) 'Pending cleanup blocked an actor request for six seconds'
    $pending=@($Sessions.result | Where-Object {$_.id -ceq $Id -and $_.generation -eq 1 -and $_.state -ceq 'disconnecting' -and $_.error -ceq $script:pendingError})
    Require ($Sessions.ok -is [bool] -and $Sessions.ok -and $pending.Count -eq 1) 'Sessions did not expose the exact pending cleanup session'
    Require ($Close.ok -is [bool] -and -not $Close.ok -and $Close.error -ceq $script:pendingError) 'Close did not promptly preserve the pending cleanup owner'
}
function Test-PendingActorResponsive([string]$Id) {
    $brokerDeadline=[DateTime]::UtcNow.AddSeconds(6);$clock=[Diagnostics.Stopwatch]::StartNew()
    $sessions=Wait-BrokerReply (Send-BrokerRequest @{action='sessions'}) -Cleanup;$sessionsMs=$clock.ElapsedMilliseconds
    Require (@($sessions.result | Where-Object {$_.id -ceq $Id -and $_.state -ceq 'disconnecting' -and $_.error -ceq $script:pendingError}).Count -eq 1) 'Pending cleanup ended before responsiveness could be verified'
    $brokerDeadline=[DateTime]::UtcNow.AddSeconds(6);$clock.Restart()
    $close=Wait-BrokerReply (Send-BrokerRequest @{action='close'} $true) -Cleanup;$closeMs=$clock.ElapsedMilliseconds
    Assert-PendingActorReplies $sessions $close $Id $sessionsMs $closeMs
    $script:activeCase['pendingActorProbe']=[ordered]@{passed=$true;sessionsMs=$sessionsMs;closeMs=$closeMs;closePreservedOwner=$true;elapsedMs=$script:caseClock.ElapsedMilliseconds}
}
function Wait-NativeCleanup([string]$Expected) {
    $deadline=[DateTime]::UtcNow.AddSeconds($script:observationCapSeconds)
    if ($script:activeCase) {$deadline=[DateTime]::UtcNow.AddMilliseconds([Math]::Max(0,$script:activeCase.observationDeadlineMs-$script:caseClock.ElapsedMilliseconds))}
    do {
        if ((Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress) -ceq $Expected) {
            try {
                $inventory=Read-NativeInventory 'after' 5000 500
                if (@(Get-Process -Name 'termterm-vpn-broker','termterm-wireguard' -ErrorAction SilentlyContinue).Count -eq 0 -or $Mode -ne 'client-death') {return $inventory}
            } catch {if ($_.Exception.Message -cne 'Native inventory did not prove no Wintun devices and no owned registered identities') {throw}}
        }
        Start-Sleep -Milliseconds 1000
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Native cleanup exceeded the twelve-minute test observation limit; no recovery performed'
}
function Read-NativeInventory([string]$Phase,[int]$ExitTimeout=10000,[int]$OutputTimeout=2000) {
    $key=$Phase+'NativeSamples';$before=if ($brokerReport.Contains($key)) {@($brokerReport[$key]).Count} else {0}
    try {& $script:nativeInventoryInner $Phase $ExitTimeout $OutputTimeout}
    finally {
        if ($script:activeCase -and @($brokerReport[$Phase+'NativeSamples']).Count -gt $before) {
            $sample=$brokerReport[$Phase+'NativeSamples'][-1]
            $script:activeCase.nativeSamples+=@([ordered]@{elapsedMs=$script:caseClock.ElapsedMilliseconds;devices=$sample.devices;registeredInterfaces=$sample.registeredInterfaces;safe=$sample.safe})
        }
    }
}
function Assert-WireGuardFixture($Value) {
    Require ($Value.version -eq 1 -and $Value.fixtureId -cmatch '^[0-9a-f]{16}$' -and $Value.kernelPeer.implementation -ceq 'Linux kernel WireGuard' -and $Value.kernelPeer.port -eq 31820) 'Unexpected kernel fixture identity'
    Require ($Value.target -ceq '10.97.0.1' -and $Value.port -eq 31823 -and $Value.payloadBytes -eq 65536) 'Unexpected bounded TCP fixture'
    Require ($Value.profile.family -ceq 'wireGuard' -and $Value.profile.config.Contains('Endpoint = 127.0.0.1:31820') -and $Value.profile.config.Contains('AllowedIPs = 10.97.0.1/32, 10.88.0.53/32') -and $Value.profile.config.Contains('DNS = 10.88.0.53, '+$Value.dnsFixture.suffix)) 'Unexpected WireGuard profile'
    Require ($Value.dnsFixture.server -ceq '10.88.0.53' -and $Value.dnsFixture.answer -ceq '10.88.0.53' -and $Value.dnsFixture.shortName -ceq 'probe' -and $Value.dnsFixture.suffix -ceq ($Value.fixtureId+'.termterm.test') -and $Value.dnsFixture.fqdn -ceq ('probe.'+$Value.dnsFixture.suffix)) 'Unexpected tunnel-only DNS fixture'
}
function Read-PeerReceipt($Fixture) {
    $value=Read-SharedJson $Fixture.peerReceiptPath
    Require ($value.fixtureId -ceq $Fixture.fixtureId -and $value.kernelPeer -is [bool] -and $value.kernelPeer -and (Test-JsonInteger $value.latestHandshakeUnix 0 ([long]::MaxValue))) 'Kernel peer receipt identity changed'
    $value
}
function Assert-WrongKeyResult($Report,[long]$Elapsed,$Before,$After) {
    Require ($Elapsed -ge 29000 -and $Elapsed -lt 45000) 'Wrong key did not fail within the bounded handshake timeout'
    Require (@($Report.events | Where-Object {$_.state -ceq 'connected'}).Count -eq 0) 'Wrong key announced connected'
    Require ($Report.cleanupPriorFailures.Count -eq 1 -and $Report.cleanupPriorFailures[0].error -ceq 'engine_failed: wireguard_peer_handshake_timeout') 'Wrong key failed for an unexpected reason'
    Require ($After.latestHandshakeUnix -eq $Before.latestHandshakeUnix) 'Wrong key acquired a kernel handshake'
}
function Test-WireGuardTcp($Fixture,[uint32]$OwnedIndex) {
    $route=@(Find-NetRoute -RemoteIPAddress $Fixture.target -ErrorAction Stop | Where-Object {$_.DestinationPrefix})
    Require ($route.Count -eq 1 -and $route[0].InterfaceIndex -eq $OwnedIndex) 'Payload target does not use the owned interface'
    $before=Read-SharedJson $Fixture.tcpReceiptPath
    $payload=[byte[]]::new($Fixture.payloadBytes);$random=[Security.Cryptography.RandomNumberGenerator]::Create();try {$random.GetBytes($payload)} finally {$random.Dispose()}
    $hasher=[Security.Cryptography.SHA256]::Create();try {$expected=$hasher.ComputeHash($payload)} finally {$hasher.Dispose()}
    $client=[Net.Sockets.TcpClient]::new()
    try {
        Require ($client.ConnectAsync($Fixture.target,$Fixture.port).Wait(5000)) 'WireGuard TCP connect timed out'
        $stream=$client.GetStream();$stream.ReadTimeout=5000;$stream.WriteTimeout=5000
        $stream.Write($payload,0,$payload.Length);$client.Client.Shutdown([Net.Sockets.SocketShutdown]::Send)
        $actual=[byte[]]::new(32);$offset=0
        while ($offset -lt $actual.Length) {$count=$stream.Read($actual,$offset,$actual.Length-$offset);Require ($count -gt 0) 'WireGuard TCP digest truncated';$offset+=$count}
        Require ([Convert]::ToBase64String($actual) -ceq [Convert]::ToBase64String($expected)) 'WireGuard payload SHA256 mismatch'
    } finally {$client.Dispose()}
    $digest=([BitConverter]::ToString($expected)).Replace('-','').ToLowerInvariant();$deadline=[DateTime]::UtcNow.AddSeconds(3)
    do {$after=Read-SharedJson $Fixture.tcpReceiptPath;if ($after.connections -gt $before.connections -and $after.bytes -ge ($before.bytes+$payload.Length) -and $after.lastSha256 -ceq $digest) {return [ordered]@{passed=$true;bytes=$payload.Length;sha256=$digest;freshKernelReceiver=$true}};Start-Sleep -Milliseconds 50} while ([DateTime]::UtcNow -lt $deadline)
    throw 'Private kernel TCP receiver did not confirm fresh matching payload'
}
function Write-RelayLossFile([string]$Path,[string]$Text) {
    $temporary=$Path+'.tmp'
    try {
        [IO.File]::WriteAllText($temporary,$Text)
        for ($attempt=0;;$attempt++) {
            try {
                if ([IO.File]::Exists($Path)) {[IO.File]::Replace($temporary,$Path,[NullString]::Value)} else {[IO.File]::Move($temporary,$Path)}
                return
            } catch [IO.IOException] {
                # A relay reader may briefly deny delete sharing.
                if (($_.Exception.GetBaseException().HResult -band 65535) -notin @(32,33) -or $attempt -ge 99) {throw}
                Start-Sleep -Milliseconds 10
            }
        }
    } finally {if ([IO.File]::Exists($temporary)) {[IO.File]::Delete($temporary)}}
}
function Set-RelayLoss([string]$Direction) {
    Require ($Direction -in @('none','both','outbound','inbound')) 'Unexpected relay loss direction'
    $script:lossGeneration++
    $value=@{fixtureId=$fixtureData.fixtureId;generation=$script:lossGeneration;direction=$Direction}
    $path=Join-Path $fixtureDirectory 'relay-loss.json'
    Write-RelayLossFile $path ($value | ConvertTo-Json -Compress)
    $deadline=[DateTime]::UtcNow.AddSeconds(3)
    do {
        $receipt=Read-SharedJson (Join-Path $fixtureDirectory 'relay-receipt.json')
        Require ($receipt.fixtureId -ceq $fixtureData.fixtureId -and -not $receipt.stopped) 'Pinned relay stopped or changed identity'
        if ($receipt.lossControl.generation -eq $script:lossGeneration -and $receipt.lossControl.direction -ceq $Direction) {return $receipt}
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Pinned relay did not apply the requested packet loss'
}
function Wait-WireGuardQuiet([string]$Id,[int]$Seconds) {
    $deadline=[DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        if (-not $script:brokerReadTask) {$script:brokerReadTask=$brokerProcess.StandardOutput.ReadLineAsync()}
        if ($script:brokerReadTask.Wait(100)) {Receive-BrokerMessage}
        Require ($script:brokerStates[$Id] -ceq 'connected' -and -not $brokerProcess.HasExited) 'Healthy WireGuard session did not remain connected'
    } while ([DateTime]::UtcNow -lt $deadline)
}
function Assert-SilentLossFailure($Failures,[long]$Detected,[long]$Terminal) {
    Require ($Detected -ge 25000 -and $Detected -lt 50000) 'Silent loss was not detected within the bounded proof timeout'
    Require ($Terminal -ge $Detected -and $Terminal -lt ($Detected+$script:observationCapSeconds*1000)) 'Silent loss cleanup exceeded the twelve-minute test observation limit'
    Require (@($Failures).Count -eq 1 -and $Failures[0].error -ceq 'engine_failed: wireguard_peer_unresponsive') 'Silent loss surfaced an unexpected failure reason'
}
function Convert-ClientDiagnostic([string]$Text) {
    $bounded=$Text.Substring(0,[Math]::Min(8192,$Text.Length))
    $codes=@('owned_adapter_still_present_cleanup_required','interface_identity_changed_after_owned_adapter_close','owned_adapter_close','adapter_still_referenced','normal_close_owned_phantom_cleanup','normal_close_journal_retry','owned_phantom_cleanup','owned_phantom_removal_not_confirmed','owned_device_is_not_phantom','unknown_phantom_status','wintun_close_global_sweep_refused','cleanup_not_complete','cleanup_required','quiesce_timeout_routes_retained','engine_failed','wireguard_peer_unresponsive','control_channel_closed')
    [ordered]@{charsObserved=$Text.Length;captureLimit=8192;atLimit=($Text.Length -ge 8192);codes=@($codes | Where-Object {$bounded -cmatch ('(?<![a-z0-9_])'+$_+'(?![a-z0-9_])')});osErrors=@([regex]::Matches($bounded,'\(os error [0-9]{1,10}\)') | ForEach-Object {$_.Value} | Select-Object -Unique);adapterTiming=@([regex]::Matches($bounded,'\b(?:expectedLuid|observedLuid|elapsedMs)=(?:Some\([0-9]{1,20}\)|None|[0-9]{1,20})(?![a-zA-Z0-9_])|\bdevnodeAbsent=(?:true|false)\b') | ForEach-Object {$_.Value} | Select-Object -Unique);elevatedBrokerStderrAvailable=$false}
}
function Test-WireGuardSilentLoss([string]$FirstId,[uint32]$FirstIndex) {
    $script:lossGeneration=0;$brokerReport['silentLossCases']=@();$id=$FirstId;$index=$FirstIndex
    foreach ($direction in @('both','outbound','inbound')) {
        $null=Set-RelayLoss 'none'
        if ($direction -ne 'both') {
            $id=[Guid]::NewGuid().ToString();$script:brokerSessions[$id]=@{generation=1};$brokerDeadline=[DateTime]::UtcNow.AddSeconds(45)
            Start-SetupAttempt $id ('reconnect-'+$direction)
            $null=Wait-BrokerReply (Send-BrokerRequest @{action='connect';id=$id;generation=1;profileId=[Guid]::NewGuid().ToString();profile=$fixtureData.profile})
            while ($script:brokerStates[$id] -ne 'connected') {Receive-BrokerMessage}
            $receipt=Wait-BrokerReply (Send-BrokerRequest @{action='networkReceipt';id=$id;generation=1});Assert-OwnedDns $receipt.result $fixtureData.dnsFixture $id;$index=$receipt.result.interfaceIndex
            Set-SetupStage 'testing-tcp'
            $null=Test-WireGuardTcp $fixtureData $index
            Complete-SetupAttempt
        } else {
            $brokerDeadline=[DateTime]::UtcNow.AddSeconds(140);$idleBefore=Read-PeerReceipt $fixtureData
            Wait-WireGuardQuiet $id 55;$idleAfter=Read-PeerReceipt $fixtureData
            Require ($idleAfter.latestHandshakeUnix -ge ($idleBefore.latestHandshakeUnix+30)) 'Idle WireGuard did not prove repeated authenticated handshakes'
            $brokerReport['healthyIdle']=[ordered]@{seconds=55;freshHandshakeAdvanceSeconds=$idleAfter.latestHandshakeUnix-$idleBefore.latestHandshakeUnix;connected=$true}
            $shortBefore=Set-RelayLoss 'both';Wait-WireGuardQuiet $id 10;$shortAfter=Set-RelayLoss 'none'
            Require ($shortAfter.droppedOutbound -gt $shortBefore.droppedOutbound) 'Short loss did not drop real ciphertext'
            $recoveryBefore=Read-PeerReceipt $fixtureData;Wait-WireGuardQuiet $id 20;$recoveryAfter=Read-PeerReceipt $fixtureData
            Require ($recoveryAfter.latestHandshakeUnix -gt $recoveryBefore.latestHandshakeUnix) 'Short loss did not recover an authenticated handshake'
            $brokerReport['shortLossRecovery']=[ordered]@{dropSeconds=10;recoverySeconds=20;connected=$true;payload=(Test-WireGuardTcp $fixtureData $index)}
        }
        $case=Start-ObservationCase $id 'silent-loss';$case['direction']=$direction
        $brokerReport.silentLossCases+=@($case)
        $before=Set-RelayLoss $direction;$case['lossAcknowledgedMs']=$script:caseClock.ElapsedMilliseconds;$case['relayBefore']=$before;$count=$brokerReport.cleanupPriorFailures.Count;$brokerDeadline=[DateTime]::UtcNow.AddSeconds(50)
        while ($script:brokerStates[$id] -notin @('disconnecting','failed')) {Receive-BrokerMessage ([uint64]::MaxValue)}
        Require ($case.Contains('disconnectingMs') -and ($case.disconnectingMs-$case.lossAcknowledgedMs) -lt 50000) 'Silent loss detection missed its separate deadline'
        $case['observationDeadlineMs']=$case.disconnectingMs+$script:observationCapSeconds*1000
        $brokerDeadline=[DateTime]::UtcNow.AddSeconds($script:observationCapSeconds)
        while ($script:brokerStates[$id] -ne 'failed') {Receive-BrokerMessage ([uint64]::MaxValue)}
        $elapsed=$script:caseClock.ElapsedMilliseconds;$failures=@($brokerReport.cleanupPriorFailures | Select-Object -Skip $count);Assert-SilentLossFailure $failures ($case.disconnectingMs-$case.lossAcknowledgedMs) ($case.terminalMs-$case.lossAcknowledgedMs)
        $case['error']=$failures[0].error
        $after=Read-SharedJson (Join-Path $fixtureDirectory 'relay-receipt.json')
        if ($direction -in @('both','outbound')) {Require ($after.droppedOutbound -gt $before.droppedOutbound -and $after.forwardedDatagrams -eq $before.forwardedDatagrams) 'Outbound loss was not a silent packet blackhole'}
        if ($direction -eq 'inbound') {Require ($after.droppedInbound -gt $before.droppedInbound -and $after.returnedDatagrams -eq $before.returnedDatagrams -and $after.forwardedDatagrams -gt $before.forwardedDatagrams) 'Inbound-only loss was not proven'}
        $inventory=Wait-NativeCleanup $baselineJson
        $case['elapsedMs']=$elapsed;$case['relayAfter']=$after;$case['baselineRestored']=$true;$case['nativeInventory']=$inventory;$case['status']='passed';$script:activeCase=$null
        $null=Set-RelayLoss 'none'
    }
}
if ($SelfTest) {
    $fixture=[pscustomobject]@{version=1;fixtureId='0123456789abcdef';kernelPeer=@{implementation='Linux kernel WireGuard';port=31820};target='10.97.0.1';port=31823;payloadBytes=65536;profile=@{family='wireGuard';config="Endpoint = 127.0.0.1:31820`nAllowedIPs = 10.97.0.1/32, 10.88.0.53/32`nDNS = 10.88.0.53, 0123456789abcdef.termterm.test"};dnsFixture=@{server='10.88.0.53';answer='10.88.0.53';shortName='probe';suffix='0123456789abcdef.termterm.test';fqdn='probe.0123456789abcdef.termterm.test'}}
    Assert-WireGuardFixture $fixture;$fixture.target='192.0.2.1';$rejected=$false;try {Assert-WireGuardFixture $fixture} catch {$rejected=$true};Require $rejected 'Unbounded fixture accepted'
    Require (Test-JsonInteger 7 0 10) 'Shared validator parameter contract changed'
    Assert-NativeInventory ([pscustomobject]@{readOnly=$true;safe=$true;devices=@();registeredTermTermInterfaces=@()}) 0
    $expected=@{events=@(@{state='failed'});cleanupPriorFailures=@(@{error='engine_failed: wireguard_peer_handshake_timeout'})};$peer=@{latestHandshakeUnix=4};Assert-WrongKeyResult $expected 30000 $peer $peer
    $expected.events=@(@{state='connected'});$rejected=$false;try {Assert-WrongKeyResult $expected 30000 $peer $peer} catch {$rejected=$true};Require $rejected 'Wrong-key connected evidence accepted'
    $expected.events=@();$expected.cleanupPriorFailures[0].error='control_channel_closed';$rejected=$false;try {Assert-WrongKeyResult $expected 30000 $peer $peer} catch {$rejected=$true};Require $rejected 'Unrelated failure accepted as wrong-key evidence'
    $script:brokerRequestId=0;$script:brokerReplies=@{};$script:brokerStates=@{};$script:brokerAllowedFailures=@{};$script:brokerSessions=@{'fixture-session'=@{generation=7}};$script:brokerReadTask=$null;$script:brokerDeadline=[DateTime]::UtcNow.AddSeconds(2)
    $script:activeCase=[ordered]@{id='fixture-session';lastObservedState='connected';events=@();nativeSamples=@()};$script:caseClock=[Diagnostics.Stopwatch]::StartNew();$script:brokerStates['fixture-session']='connected'
    $script:brokerProcess=[pscustomobject]@{StandardOutput=[IO.StringReader]::new('{"kind":"quiesceRequired","id":"fixture-session","generation":7}');StandardInput=[IO.StringWriter]::new()};Receive-BrokerMessage
    $ack=$script:brokerProcess.StandardInput.ToString() | ConvertFrom-Json;Require ($ack.action -ceq 'quiesced' -and $ack.generation -eq 7) 'Shared quiescence function parameters changed'
    Require ($script:activeCase.Contains('quiesceAckSentMs')) 'Local wrapper lost the actual quiescence acknowledgement timestamp';$script:activeCase=$null
    Assert-SilentLossFailure @(@{error='engine_failed: wireguard_peer_unresponsive'}) 45000 645000
    foreach ($bad in @(@{error='control_channel_closed';detected=45000;terminal=46000},@{error='engine_failed: wireguard_peer_unresponsive';detected=51000;terminal=52000},@{error='engine_failed: wireguard_peer_unresponsive';detected=45000;terminal=766000})) {
        $rejected=$false;try {Assert-SilentLossFailure @(@{error=$bad.error}) $bad.detected $bad.terminal} catch {$rejected=$true};Require $rejected 'Unrelated, late detection or incomplete cleanup evidence accepted'
    }
    $diagnostic=Convert-ClientDiagnostic 'password=private-secret private_key=AAA owned_adapter_still_present_cleanup_required elapsedMs=30001 (os error 5)'
    Require ($diagnostic.codes -contains 'owned_adapter_still_present_cleanup_required' -and $diagnostic.osErrors.Count -eq 1 -and $diagnostic.adapterTiming -contains 'elapsedMs=30001' -and ($diagnostic | ConvertTo-Json -Compress) -notmatch 'private-secret|private_key|AAA') 'Client diagnostic leaked raw stderr or lost fixed cleanup evidence'
    $diagnostic=Convert-ClientDiagnostic (('x'*8192)+' control_channel_closed');Require ($diagnostic.atLimit -and $diagnostic.codes.Count -eq 0) 'Client diagnostic exceeded capture bound'
    $diagnostic=Convert-ClientDiagnostic 'expectedLuid=Some(17) observedLuid=Some(18) expectedLuid=None';Require ($diagnostic.adapterTiming.Count -eq 3) 'Optional adapter identity diagnostic was lost'
    $sessions=@{ok=$true;result=@(@{id='fixture-session';generation=1;state='disconnecting';error=$script:pendingError})};$close=@{ok=$false;error=$script:pendingError}
    Assert-PendingActorReplies $sessions $close 'fixture-session' 10 20
    foreach ($bad in @('slow','wrong-error','closed-owner','released-session')) {
        $elapsed=10;$close=@{ok=$false;error=$script:pendingError};$sessions.result[0].state='disconnecting'
        if ($bad -eq 'slow') {$elapsed=6000};if ($bad -eq 'wrong-error') {$close.error='cleanup_required'};if ($bad -eq 'closed-owner') {$close.ok=$true};if ($bad -eq 'released-session') {$sessions.result[0].state='disconnected'}
        $rejected=$false;try {Assert-PendingActorReplies $sessions $close 'fixture-session' $elapsed 20} catch {$rejected=$true};Require $rejected 'Invalid pending responsiveness evidence accepted'
    }
    $script:brokerRequestId=0;$script:brokerReplies=@{};$script:brokerAllowedFailures=@{};$script:brokerReadTask=$null;$script:brokerSessions=@{'fixture-session'=@{generation=1}};$script:brokerStates=@{'fixture-session'='connected'};$script:brokerReport=[ordered]@{events=@();cleanupPriorFailures=@();ok=$true}
    $null=Start-ObservationCase 'fixture-session' 'self-test';$brokerDeadline=[DateTime]::UtcNow.AddSeconds(2)
    $frames=@(@{kind='session';session=@{id='fixture-session';generation=1;state='disconnecting';error=$script:pendingError}},@{kind='reply';requestId=1;ok=$true;result=@(@{id='fixture-session';generation=1;state='disconnecting';error=$script:pendingError})},@{kind='reply';requestId=2;ok=$false;error=$script:pendingError})
    $inputText=(@($frames | ForEach-Object {$_ | ConvertTo-Json -Depth 5 -Compress}) -join "`n")
    $script:brokerProcess=[pscustomobject]@{StandardOutput=[IO.StringReader]::new($inputText);StandardInput=[IO.StringWriter]::new()};Receive-BrokerMessage
    $requests=@($script:brokerProcess.StandardInput.ToString().Trim() -split "`r?`n" | ForEach-Object {$_ | ConvertFrom-Json})
    Require ($script:activeCase.pendingActorProbe.passed -and $script:activeCase.pendingPublicError -ceq $script:pendingError -and $requests.Count -eq 2 -and $requests[0].action -ceq 'sessions' -and $requests[1].action -ceq 'close') 'Actual pending stream did not preserve the owner and probe both actor actions';$script:activeCase=$null
    $script:traceSetupRequests=$true;$script:setupRequestActions=@{'39'=@{action='quiesced'};'40'=@{action='close'}};$script:brokerRequestId=40;$script:brokerReplies=@{};$script:brokerAllowedFailures=@{'40'=$true};$script:brokerSessions=@{'old-both'=@{generation=1};'old-outbound'=@{generation=1};'new-inbound'=@{generation=1}};$script:brokerStates=@{'old-both'='failed';'old-outbound'='failed';'new-inbound'='connecting'};$script:brokerReport=[ordered]@{events=@();setupAttempts=@();cleanupPriorFailures=@();ok=$true}
    $frames=@(@{kind='reply';requestId=39;ok=$true;result=@{}},@{kind='session';session=@{id='old-both';generation=0;state='connected'}},@{kind='reply';requestId=40;ok=$false;error=$script:pendingError},@{kind='session';session=@{id='old-outbound';generation=0;state='connecting'}},@{kind='reply';requestId=41;ok=$true;result=@{}},@{kind='session';session=@{id='new-inbound';generation=1;state='connected'}},@{kind='reply';requestId=42;ok=$true;result=@{}})
    $inputText=(@($frames | ForEach-Object {$_ | ConvertTo-Json -Depth 5 -Compress}) -join "`n")
    $script:brokerProcess=[pscustomobject]@{StandardOutput=[IO.StringReader]::new($inputText);StandardInput=[IO.StringWriter]::new();Id=4242;HasExited=$false};$script:brokerReadTask=$script:brokerProcess.StandardOutput.ReadLineAsync();$brokerDeadline=[DateTime]::UtcNow.AddSeconds(2)
    Start-SetupAttempt 'new-inbound' 'reconnect-inbound';$attempt=$script:activeSetup
    $request=Send-BrokerRequest @{action='connect';id='new-inbound';generation=1};$null=Wait-BrokerReply $request
    while ($script:brokerStates['new-inbound'] -ne 'connected') {Receive-BrokerMessage}
    $null=Wait-BrokerReply (Send-BrokerRequest @{action='networkReceipt';id='new-inbound';generation=1});Complete-SetupAttempt
    Require ($request -eq 41 -and $attempt.status -ceq 'passed' -and $attempt.requests.Count -eq 2 -and $attempt.requests[1].requestId -eq 42 -and $attempt.replies.Count -eq 4 -and $attempt.readerPresentBefore -and $null -eq $script:brokerReadTask -and $script:brokerStates['old-both'] -ceq 'failed' -and $script:brokerStates['old-outbound'] -ceq 'failed' -and @($attempt.events | Where-Object {$_.currentSetup}).Count -eq 1 -and $attempt.Contains('connectReplyMs') -and $attempt.Contains('connectedMs') -and $attempt.Contains('networkReceiptReplyMs')) 'Interleaved completed sessions, pending reader or old replies corrupted third setup tracing'
    $script:traceSetupRequests=$false;$script:activeSetup=$null
    if (-not ('RelayLossSelfTest' -as [type])) {Add-Type -TypeDefinition 'using System;using System.Threading;using System.Threading.Tasks;public static class RelayLossSelfTest {public static Task ReleaseAfter(IDisposable reader,int milliseconds) {return Task.Factory.StartNew(()=>{Thread.Sleep(milliseconds);reader.Dispose();});}}'}
    $directory=Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'));$null=New-Item -ItemType Directory -Path $directory
    $path=Join-Path $directory 'relay-loss.json';$temporary=$path+'.tmp';$reader=$null
    try {
        Write-RelayLossFile $path 'initial';$acl=Get-Acl -LiteralPath $path;$acl.SetAccessRuleProtection($true,$true);Set-Acl -LiteralPath $path -AclObject $acl;$sddl=(Get-Acl -LiteralPath $path).Sddl
        Write-RelayLossFile $path 'original'
        Require ([IO.File]::ReadAllText($path) -ceq 'original' -and (Get-Acl -LiteralPath $path).Sddl -ceq $sddl -and -not [IO.File]::Exists($temporary)) 'Atomic relay control create/replace changed its ACL or left a temporary file'
        [IO.File]::WriteAllText($temporary,'replacement');$reader=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
        $rejected=$false;try {Move-Item -LiteralPath $temporary -Destination $path -Force} catch {$rejected=$_.FullyQualifiedErrorId -like 'MoveFileInfoItemIOError,*'}
        Require ($rejected -and [IO.File]::ReadAllText($path) -ceq 'original') 'Delete-sharing lock did not reproduce the original Move-Item failure'
        [IO.File]::Delete($temporary);$unlock=[RelayLossSelfTest]::ReleaseAfter($reader,200);$watch=[Diagnostics.Stopwatch]::StartNew()
        Write-RelayLossFile $path 'replacement'
        Require ($unlock.Wait(1000) -and $watch.ElapsedMilliseconds -ge 100 -and $watch.ElapsedMilliseconds -lt 3000 -and [IO.File]::ReadAllText($path) -ceq 'replacement' -and (Get-Acl -LiteralPath $path).Sddl -ceq $sddl -and -not [IO.File]::Exists($temporary)) 'Transient relay reader lock was not retried atomically with its ACL preserved';$reader.Dispose();$reader=$null
        $reader=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite);$watch.Restart();$rejected=$false
        try {Write-RelayLossFile $path 'must-not-replace'} catch {$rejected=($_.Exception.GetBaseException().HResult -band 65535) -eq 32}
        Require ($rejected -and $watch.ElapsedMilliseconds -lt 3000 -and [IO.File]::ReadAllText($path) -ceq 'replacement' -and -not [IO.File]::Exists($temporary)) 'Permanent relay reader lock was swallowed, unbounded or changed the original control';$reader.Dispose();$reader=$null
        [IO.File]::SetAttributes($path,[IO.FileAttributes]::ReadOnly);$watch.Restart();$rejected=$false
        try {Write-RelayLossFile $path 'must-not-replace'} catch {$rejected=($_.Exception.GetBaseException().HResult -band 65535) -eq 5} finally {[IO.File]::SetAttributes($path,[IO.FileAttributes]::Normal)}
        Require ($rejected -and $watch.ElapsedMilliseconds -lt 1000 -and [IO.File]::ReadAllText($path) -ceq 'replacement' -and -not [IO.File]::Exists($temporary)) 'Unrelated relay permission failure was retried or swallowed'
        [IO.File]::WriteAllText($path,'{"generation":7}')
        Require ((Read-SharedJson $path).generation -eq 7) 'Shared JSON reader changed a complete snapshot'
        $reader=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None);$unlock=[RelayLossSelfTest]::ReleaseAfter($reader,200);$watch.Restart()
        Require ((Read-SharedJson $path).generation -eq 7 -and $unlock.Wait(1000) -and $watch.ElapsedMilliseconds -lt 3000) 'Shared JSON reader did not recover a transient sharing lock';$reader.Dispose();$reader=$null
        [IO.File]::WriteAllText($path,'{');$watch.Restart();$rejected=$false;try {$null=Read-SharedJson $path} catch {$rejected=$true}
        Require ($rejected -and $watch.ElapsedMilliseconds -lt 1000) 'Malformed JSON was retried or accepted'
        $watch.Restart();$rejected=$false;try {$null=Read-SharedJson ($path+'.missing')} catch {$rejected=($_.Exception.GetBaseException().HResult -band 65535) -eq 2}
        Require ($rejected -and $watch.ElapsedMilliseconds -lt 3000) 'Missing shared JSON did not fail within its bound'
    } finally {if ($reader) {$reader.Dispose()};[IO.File]::Delete($temporary);[IO.File]::Delete($path);[IO.Directory]::Delete($directory)}
    [ordered]@{selfTest=$true;passed=32;sharedFunctions=21;privilegedActions=$false;networkActions=$false} | ConvertTo-Json -Compress
    return
}
Require ($Helper -and $ExpectedManifestSha256 -and $Fixtures -and $ExpectedFixturesSha256) 'Reviewed helper, manifest and private fixture hashes required'
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
Require (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) 'Run unelevated; only the fixed helper requests UAC'
$helperPath=(Resolve-Path -LiteralPath $Helper).Path;$bundle=Split-Path -Parent $helperPath
Require ([IO.Path]::GetFileName($helperPath) -ceq 'termterm-vpn-broker.exe' -and $bundle -ceq $PSScriptRoot) 'Use the frozen acceptance bundle'
$fixturePath=(Resolve-Path -LiteralPath $Fixtures).Path;$fixtureDirectory=Split-Path -Parent $fixturePath
Require ($fixtureDirectory.StartsWith([IO.Path]::GetFullPath((Join-Path $bundle '../../../.lab/')),[StringComparison]::OrdinalIgnoreCase)) 'Private fixture must remain in this worktree lab'
$reportPath=Join-Path $bundle "wireguard-v1-$Mode-result.json"
Require (-not (Test-Path -LiteralPath $reportPath)) 'Preserve earlier acceptance evidence; use a new bundle'
$brokerProcess=$null;$baselineJson=$null;$bundleVerified=$false;$ManyRoutes=$false
$brokerReport=[ordered]@{ok=$false;mode=$Mode;protocol=2;peer='Linux kernel WireGuard';events=@();setupAttempts=@();cleanupPriorFailures=@();cleanupEvidence='pending';companyNetworkTested=$false;recoveryPerformed=$false}
$script:traceSetupRequests=$true;$script:setupRequestActions=@{};$script:activeSetup=$null
$script:brokerReplies=@{};$script:brokerStates=@{};$script:brokerSessions=@{};$script:brokerAllowedFailures=@{};$script:brokerRequestId=0;$script:brokerReadTask=$null
try {
    Require (@(Get-Process -Name 'termterm-vpn-broker' -ErrorAction SilentlyContinue).Count -eq 0) 'Another development broker is active; preserve it'
    $manifest=(Read-LockedFile (Join-Path $bundle 'manifest.json') $ExpectedManifestSha256) | ConvertFrom-Json
    Require ($manifest.protocolVersion -eq 2 -and $manifest.developmentOnly -is [bool] -and $manifest.developmentOnly -and $manifest.mode -ceq $Mode) 'Matching protocol 2 development manifest required'
    foreach ($name in @('termterm-vpn-broker.exe','termterm-wireguard.exe','wintun.dll','validate-wireguard-v1.ps1','validate-full-broker-v5.ps1','validate-dns-v1.ps1')) {Require ($name -in @($manifest.files.name)) 'Required reviewed bundle member missing'}
    foreach ($file in $manifest.files) {Require ([IO.Path]::GetFileName($file.name) -ceq $file.name) 'Invalid manifest member';$null=Read-LockedFile (Join-Path $bundle $file.name) $file.sha256}
    $helperHash=($manifest.files | Where-Object {$_.name -ceq 'termterm-vpn-broker.exe'}).sha256
    $audit=Read-SharedJson (Join-Path $bundle 'dns-journal-audit-before.json')
    Require ($audit.ok -is [bool] -and $audit.ok -and $audit.readOnly -is [bool] -and $audit.readOnly -and $audit.recoveryPerformed -is [bool] -and -not $audit.recoveryPerformed -and $audit.phase -ceq 'before' -and $audit.helperSha256 -ceq $helperHash -and $audit.protectedJournalCount -eq 0) 'Separate reviewed empty-journal audit required'
    $fixtureData=(Read-LockedFile $fixturePath $ExpectedFixturesSha256) | ConvertFrom-Json;Assert-WireGuardFixture $fixtureData;Require ($manifest.fixtureId -ceq $fixtureData.fixtureId) 'Frozen fixture identity changed'
    foreach ($entry in @(@('peerReceiptPath','peer-receipt.json'),@('tcpReceiptPath','tcp-receipt.json'))) {Require ([IO.Path]::GetFullPath($fixtureData.($entry[0])) -ceq (Join-Path $fixtureDirectory $entry[1])) 'Receipt escaped pinned fixture'}
    Require ([IO.Path]::GetFullPath($fixtureData.dnsFixture.receiptPath) -ceq (Join-Path $fixtureDirectory 'dns-receipt.json')) 'DNS receipt escaped pinned fixture'
    Require (-not (Test-Path -LiteralPath (Join-Path $fixtureDirectory 'stop')) -and -not (Test-Path -LiteralPath (Join-Path $fixtureDirectory 'stopped.json'))) 'Fixture has stopped; prepare fresh evidence'
    $relay=Read-SharedJson (Join-Path $fixtureDirectory 'relay-receipt.json')
    Require ($relay.fixtureId -ceq $fixtureData.fixtureId -and $relay.loopbackOnly -is [bool] -and $relay.loopbackOnly -and -not $relay.stopped) 'Pinned loopback relay is not live'
    $peerBefore=Read-PeerReceipt $fixtureData;$bundleVerified=$true
    $brokerReport['beforeNativeInventory']=Read-NativeInventory 'before'
    $baselineJson=Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress
    $baselinePath=Join-Path $fixtureDirectory ('windows-baseline-'+(Split-Path -Leaf $bundle)+'-'+$Mode+'.json');Require (-not (Test-Path -LiteralPath $baselinePath)) 'Preserve previous private baseline'
    [IO.File]::WriteAllText($baselinePath,$baselineJson)
    $brokerProcess=Start-DnsClient;$stderrBuffer=[char[]]::new(8192);$stderrTask=$brokerProcess.StandardError.ReadBlockAsync($stderrBuffer,0,$stderrBuffer.Length);$brokerDeadline=[DateTime]::UtcNow.AddSeconds(180)
    $hello=Wait-BrokerReply (Send-BrokerRequest @{action='hello';vaultId=[Guid]::NewGuid().ToString()})
    Require ($hello.result.version -eq 2 -and $hello.result.networkLease -is [bool] -and $hello.result.networkLease -and $hello.result.quiesce -is [bool] -and $hello.result.quiesce) 'No protocol 2 fallback is permitted'
    $id=[Guid]::NewGuid().ToString();$script:brokerSessions[$id]=@{generation=1};Start-Sleep -Milliseconds 1100
    $profile=if ($Mode -eq 'wrong-key') {$fixtureData.wrongKeyProfile} else {$fixtureData.profile}
    Start-SetupAttempt $id 'initial'
    $clock=[Diagnostics.Stopwatch]::StartNew();$null=Wait-BrokerReply (Send-BrokerRequest @{action='connect';id=$id;generation=1;profileId=[Guid]::NewGuid().ToString();profile=$profile})
    if ($Mode -eq 'wrong-key') {
        $case=Start-ObservationCase $id 'wrong-key';$case.lastObservedState='connecting';$brokerReport['terminationCase']=$case;$brokerDeadline=[DateTime]::UtcNow.AddSeconds(45)
        while ($script:brokerStates[$id] -notin @('disconnecting','failed')) {Receive-BrokerMessage ([uint64]::MaxValue)}
        Require ($case.Contains('disconnectingMs')) 'Wrong key did not expose bounded handshake rejection before cleanup'
        $case['observationDeadlineMs']=$case.disconnectingMs+$script:observationCapSeconds*1000;$brokerDeadline=[DateTime]::UtcNow.AddSeconds($script:observationCapSeconds)
        while ($script:brokerStates[$id] -ne 'failed') {Receive-BrokerMessage ([uint64]::MaxValue)}
        $peerAfter=Read-PeerReceipt $fixtureData;Assert-WrongKeyResult $brokerReport $case.disconnectingMs $peerBefore $peerAfter
        $brokerReport['wrongKeyRejectedWithoutConnected']=$true
        $script:activeSetup['status']='expected-rejection';Set-SetupStage 'handshake-rejected';$script:activeSetup=$null
    } else {
        Wait-AllConnected
        $receipt=Wait-BrokerReply (Send-BrokerRequest @{action='networkReceipt';id=$id;generation=1})
        Assert-OwnedDns $receipt.result $fixtureData.dnsFixture $id
        $brokerReport['payload']=Test-WireGuardTcp $fixtureData $receipt.result.interfaceIndex
        $resolver=Test-NormalResolver $fixtureData.dnsFixture;$brokerReport['resolver']=$resolver;Require $resolver.passed 'Fresh normal Windows resolver traffic was not proven'
        $peerAfter=Read-PeerReceipt $fixtureData
        Require ($peerAfter.latestHandshakeUnix -gt $peerBefore.latestHandshakeUnix -and $peerAfter.receivedBytes -gt $peerBefore.receivedBytes -and $peerAfter.sentBytes -gt $peerBefore.sentBytes) 'Kernel peer did not confirm a fresh handshake and encrypted traffic'
        $brokerReport['freshKernelHandshakeAndTraffic']=$true
        $brokerReport['ownedInterfaceVerified']=$true
        Complete-SetupAttempt
        if ($Mode -eq 'silent-loss') {Test-WireGuardSilentLoss $id $receipt.result.interfaceIndex}
    }
    $cleanupClock=[Diagnostics.Stopwatch]::StartNew()
    if ($Mode -eq 'client-death') {
        $case=Start-ObservationCase $id 'client-death';$brokerReport['terminationCase']=$case;$case['ownerKilledMs']=$script:caseClock.ElapsedMilliseconds
        $brokerProcess.Kill();Require ($brokerProcess.WaitForExit(10000)) 'Exact validation owner did not exit';$brokerReport['cleanupEvidence']='pending-owner-death'
    } else {
        $brokerDeadline=[DateTime]::UtcNow.AddSeconds(25)
        if ($Mode -eq 'normal') {
            $case=Start-ObservationCase $id 'normal-disconnect';$brokerReport['terminationCase']=$case;$brokerDeadline=[DateTime]::UtcNow.AddSeconds($script:observationCapSeconds)
            $null=Wait-BrokerReply (Send-BrokerRequest @{action='disconnect';id=$id;generation=1})
            while ($script:brokerStates[$id] -notin @('disconnected','failed')) {Receive-BrokerMessage ([uint64]::MaxValue)}
            Require ($script:brokerStates[$id] -ceq 'disconnected') 'Normal disconnect ended in a failure state'
            $brokerReport['explicitDisconnectVerified']=$true
        }
        $brokerDeadline=[DateTime]::UtcNow.AddSeconds(25)
        Confirm-BrokerCleanup (Wait-BrokerReply (Send-BrokerRequest @{action='close'}) -Cleanup)
        $brokerProcess.StandardInput.Close();Require ($brokerProcess.WaitForExit(10000) -and $brokerProcess.ExitCode -eq 0) 'Normal validation owner exit failed'
    }
    $brokerReport['afterNativeInventory']=Wait-NativeCleanup $baselineJson;$brokerReport['cleanupElapsedMs']=$cleanupClock.ElapsedMilliseconds;$brokerReport['baselineRestored']=$true
    if ($script:activeCase) {$script:activeCase['status']='passed';$script:activeCase=$null}
    if ($Mode -eq 'client-death') {$brokerReport['cleanupEvidence']='owner-death-baseline-and-native-inventory';$brokerReport['requiresSeparateElevatedJournalAudit']=$true}
    $brokerReport['ok']=$true
} catch {
    $brokerReport['error']=$_.Exception.Message
    if ($script:activeSetup) {$script:activeSetup['status']='failed';$script:activeSetup['failure']=[ordered]@{stage=$script:activeSetup.stage;elapsedMs=$script:setupClock.ElapsedMilliseconds;ownerHasExited=if ($brokerProcess) {$brokerProcess.HasExited} else {$null};requestCounter=$script:brokerRequestId;readerPresent=($null -ne $script:brokerReadTask);readerIsCompleted=if ($script:brokerReadTask) {$script:brokerReadTask.IsCompleted} else {$null};stack=$_.ScriptStackTrace}}
    throw
} finally {
    if ($brokerProcess) {
        try {if (-not $brokerProcess.HasExited) {if ($script:activeCase) {$script:activeCase['ownerKilledMs']=$script:caseClock.ElapsedMilliseconds};$brokerProcess.Kill();Require ($brokerProcess.WaitForExit(10000)) 'Retained validation owner cleanup timed out'}} catch {$brokerReport['retainedClientCleanupFailed']=$true;$brokerReport['ok']=$false}
        finally {
            try {
                if ($stderrTask -and $stderrTask.Wait(2000)) {
                    $count=$stderrTask.GetAwaiter().GetResult();$diagnostic=Convert-ClientDiagnostic ([string]::new($stderrBuffer,0,$count));$brokerReport['clientDiagnostic']=$diagnostic
                    [IO.File]::WriteAllText((Join-Path $fixtureDirectory ('windows-client-diagnostic-'+(Split-Path -Leaf $bundle)+'.json')),($diagnostic | ConvertTo-Json -Depth 4))
                } else {$brokerReport['clientDiagnosticIncomplete']=$true}
            } catch {$brokerReport['clientDiagnosticCaptureFailed']=$true}
            $brokerProcess.Dispose()
        }
    }
    if ($Mode -eq 'silent-loss' -and $bundleVerified) {try {$null=Set-RelayLoss 'none'} catch {$brokerReport['relayRestoreFailed']=$true;$brokerReport['ok']=$false}}
    $capReached=$script:activeCase -and $script:caseClock.ElapsedMilliseconds -ge $script:activeCase.observationDeadlineMs
    if ($bundleVerified -and $baselineJson) {try {$brokerReport['finalBaselineRestored']=if ($capReached) {(Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress) -ceq $baselineJson} else {Wait-DnsBaseline $baselineJson}} catch {$brokerReport['finalBaselineRestored']=$false};if (-not $brokerReport.finalBaselineRestored) {$brokerReport['ok']=$false}}
    if ($bundleVerified -and $baselineJson -and -not $brokerReport.Contains('afterNativeInventory')) {
        try {$brokerReport['failureFinalNativeInventory']=if ($capReached) {$brokerReport['observationCapReached']=$true;Read-NativeInventory 'failure-final' 1000 500} else {Wait-NativeCleanup $baselineJson}} catch {$brokerReport['failureFinalNativeCleanupFailed']=$true;$brokerReport['ok']=$false}
    }
    foreach ($stream in $locks) {$stream.Dispose()}
    [IO.File]::WriteAllText($reportPath,($brokerReport | ConvertTo-Json -Depth 14));Write-Output "WireGuard acceptance report: $reportPath"
}
Require $brokerReport.ok 'WireGuard native acceptance failed; preserve the redacted report and private baseline'
