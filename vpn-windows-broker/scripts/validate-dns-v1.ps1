param(
    [string]$Helper,
    [string]$ExpectedManifestSha256,
    [string]$Fixtures,
    [string]$ExpectedFixturesSha256,
    [string]$ExpectedValidatorSha256,
    [ValidateSet('normal','client-death')][string]$Mode='normal',
    [switch]$RouteExclusions,
    [switch]$FullTunnel,
    [switch]$ManyRoutes,
    [switch]$NestedRoutes,
    [switch]$SelfTest
)
$ErrorActionPreference='Stop'
$validatorPath=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'validate-full-broker-v5.ps1')).Path
$validatorBytes=[IO.File]::ReadAllBytes($validatorPath)
$hasher=[Security.Cryptography.SHA256]::Create()
try {$validatorHash=([BitConverter]::ToString($hasher.ComputeHash($validatorBytes))).Replace('-','').ToLowerInvariant()} finally {$hasher.Dispose()}
if (-not $SelfTest -and $validatorHash -ne $ExpectedValidatorSha256) {throw 'Reviewed shared validator hash changed'}
$parseErrors=$null
$ast=[Management.Automation.Language.Parser]::ParseInput([Text.Encoding]::UTF8.GetString($validatorBytes),$validatorPath,[ref]$null,[ref]$parseErrors)
if ($parseErrors) {throw 'Shared validator did not parse'}
$functionNames=@('Read-LockedFile','Assert-NativeInventory','Send-BrokerRequest','Resolve-FixtureAnswer','Receive-BrokerMessage','Wait-BrokerReply','Confirm-BrokerCleanup','Assert-PendingCleanupSessions','Wait-BrokerCleanup','Wait-AllConnected')
foreach ($name in $functionNames) {
    $definitions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$false) | Where-Object {$_.Name -ceq $name})
    if ($definitions.Count -ne 1) {throw 'Reviewed shared validator function missing or duplicated'}
    . ([scriptblock]::Create($definitions[0].Extent.Text))
}
function Require($Value,[string]$Message) {if (-not $Value) {throw $Message}}
function Read-SharedJson([string]$Path) {
    for ($attempt=0;;$attempt++) {
        try {
            $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
            try {
                $reader=[IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false,$true))
                try {$text=$reader.ReadToEnd()} finally {$reader.Dispose()}
            } finally {$stream.Dispose()}
            break
        } catch [IO.IOException] {
            if (($_.Exception.GetBaseException().HResult -band 65535) -notin @(2,3,32,33) -or $attempt -ge 99) {throw}
            Start-Sleep -Milliseconds 10
        }
    }
    $text | ConvertFrom-Json
}
function Test-JsonInteger($Value,[long]$Minimum,[long]$Maximum) {
    ($Value -is [int] -or $Value -is [long]) -and $Value -ge $Minimum -and $Value -le $Maximum
}
function Wait-InventoryOutput($Task,[int]$Timeout) {
    Require ($Task.Wait($Timeout)) 'Read-only native inventory output timed out'
    $Task.GetAwaiter().GetResult()
}
function Invoke-BoundedInventoryProbe([Diagnostics.Process]$Probe,[string]$Phase,[int]$ExitTimeout=10000,[int]$OutputTimeout=2000) {
    $started=$false;$failed=$false;$cleanupFailed=$false
    try {
        Require ($Probe.Start()) 'Read-only native inventory did not start';$started=$true
        $stdout=$Probe.StandardOutput.ReadToEndAsync();$stderr=$Probe.StandardError.ReadToEndAsync()
        Require ($Probe.WaitForExit($ExitTimeout)) 'Read-only native inventory timed out'
        $text=Wait-InventoryOutput $stdout $OutputTimeout
        $errorText=Wait-InventoryOutput $stderr $OutputTimeout
        [pscustomobject]@{text=$text;stderr=$errorText;exitCode=$Probe.ExitCode}
    } catch {$failed=$true;throw}
    finally {
        if ($started) {
            try {
                if (-not $Probe.HasExited) {
                    $Probe.Kill()
                    Require ($Probe.WaitForExit(2000)) 'Retained inventory process did not exit after termination'
                }
            } catch {$cleanupFailed=$true}
            try {$Probe.StandardOutput.Dispose();$Probe.StandardError.Dispose()} catch {$cleanupFailed=$true}
        }
        try {$Probe.Dispose()} catch {$cleanupFailed=$true}
        if ($cleanupFailed) {
            $brokerReport[$Phase+'NativeCleanupFailed']=$true
            if (-not $failed) {throw 'Retained inventory process cleanup failed'}
        }
    }
}
function Read-NativeInventory([string]$Phase,[int]$ExitTimeout=10000,[int]$OutputTimeout=2000) {
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$helperPath;$start.Arguments='--wintun-sweep-preflight';$start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    $probe=[Diagnostics.Process]::new();$probe.StartInfo=$start
    $result=Invoke-BoundedInventoryProbe $probe $Phase $ExitTimeout $OutputTimeout
    $brokerReport[$Phase+'NativeExitCode']=$result.exitCode;$brokerReport[$Phase+'NativeStderr']=$result.stderr
    $value=$result.text | ConvertFrom-Json
    if (-not $brokerReport.Contains($Phase+'NativeSamples')) {$brokerReport[$Phase+'NativeSamples']=@()}
    $brokerReport[$Phase+'NativeSamples']+=@([ordered]@{exitCode=$result.exitCode;readOnly=($value.readOnly -is [bool] -and $value.readOnly);safe=($value.safe -is [bool] -and $value.safe);devices=if ($value.devices -is [Array]) {$value.devices.Count} else {-1};registeredInterfaces=if ($value.registeredTermTermInterfaces -is [Array]) {$value.registeredTermTermInterfaces.Count} else {-1}})
    Assert-NativeInventory $value $result.exitCode
    $value
}
function Assert-DnsFixture($Value) {
    Require ($Value.server -ceq '10.88.0.53' -and $Value.answer -ceq '10.88.0.53') 'Unexpected disposable DNS address'
    Require ($Value.suffix -cmatch '^[0-9a-f]{16}\.termterm\.test$' -and $Value.shortName -ceq 'probe' -and $Value.fqdn -ceq ('probe.'+$Value.suffix)) 'Unexpected disposable DNS name'
    Require ($Value.profile.family -ceq 'openVpn' -and $Value.profile.config.Contains('remote 127.0.0.1 31199')) 'Unexpected disposable DNS profile'
}
function Read-DnsBaseline {
    [ordered]@{
        addresses=@(Get-NetIPAddress | Select-Object InterfaceIndex,AddressFamily,IPAddress,PrefixLength,PrefixOrigin,SuffixOrigin | Sort-Object InterfaceIndex,AddressFamily,IPAddress)
        routes=@(Get-NetRoute | Select-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop,RouteMetric,Protocol | Sort-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop)
        interfaces=@(Get-NetIPInterface | Select-Object InterfaceIndex,AddressFamily,NlMtu,InterfaceMetric,Dhcp,RouterDiscovery,Forwarding | Sort-Object InterfaceIndex,AddressFamily)
        dns=@(Get-DnsClientServerAddress | Select-Object InterfaceIndex,AddressFamily,@{Name='Servers';Expression={$_.ServerAddresses -join ','}} | Sort-Object InterfaceIndex,AddressFamily)
        suffixes=@(Get-DnsClient | Select-Object InterfaceIndex,ConnectionSpecificSuffix,@{Name='SearchList';Expression={$_.ConnectionSpecificSuffixSearchList -join ','}},RegisterThisConnectionsAddress,UseSuffixWhenRegistering | Sort-Object InterfaceIndex)
        globalSuffix=Get-DnsClientGlobalSetting | Select-Object SuffixSearchList,UseDevolution,DevolutionLevel
    }
}
function Wait-DnsBaseline([string]$Expected) {
    $deadline=[DateTime]::UtcNow.AddSeconds(30)
    do {
        if ((Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress) -ceq $Expected) {return $true}
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    $false
}
function Wait-NativeCleanup([string]$Expected) {
    $budget=if ($ManyRoutes) {60} else {30}
    $deadline=[DateTime]::UtcNow.AddSeconds($budget)
    do {
        if ((Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress) -ceq $Expected) {
            $remaining=[int]($deadline-[DateTime]::UtcNow).TotalMilliseconds
            if ($remaining -le 3500) {break}
            try {return Read-NativeInventory 'after' ([Math]::Min(5000,$remaining-3000)) 500}
            catch {if ($_.Exception.Message -cne 'Native inventory did not prove no Wintun devices and no owned registered identities') {throw}}
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Network baseline and strict native cleanup were not both confirmed within $budget seconds"
}
function Assert-OwnedDns($Receipt,$Fixture,[string]$SessionId) {
    Require ($Receipt.id -ceq $SessionId -and $Receipt.generation -eq 1) 'Network receipt session mismatch'
    $guid=[Guid]::Empty
    Require ([Guid]::TryParse([string]$Receipt.guid,[ref]$guid) -and $guid -ne [Guid]::Empty) 'Network receipt GUID invalid'
    Require ($Receipt.luid -match '^[1-9][0-9]*$' -and (Test-JsonInteger $Receipt.interfaceIndex 1 ([uint32]::MaxValue))) 'Network receipt interface invalid'
    $adapters=@(Get-NetAdapter -IncludeHidden | Where-Object {[Guid]$_.InterfaceGuid -eq $guid})
    Require ($adapters.Count -eq 1 -and $adapters[0].InterfaceIndex -eq $Receipt.interfaceIndex) 'Exact owned adapter mapping failed'
    $servers=@((Get-DnsClientServerAddress -InterfaceIndex $Receipt.interfaceIndex -AddressFamily IPv4).ServerAddresses)
    $client=Get-DnsClient -InterfaceIndex $Receipt.interfaceIndex
    Require ($servers.Count -eq 1 -and $servers[0] -ceq $Fixture.server -and $client.ConnectionSpecificSuffix -ceq $Fixture.suffix) 'Adapter DNS or suffix readback differs'
    Require (@($Receipt.dns.servers).Count -eq 1 -and $Receipt.dns.servers[0] -ceq $Fixture.server -and $Receipt.dns.searchDomain -ceq $Fixture.suffix) 'Broker DNS readback differs'
    Require ($null -ne $Receipt.dns.searchList -and @($Receipt.dns.searchList).Count -eq 1 -and $Receipt.dns.searchList[0] -ceq $Fixture.suffix -and $null -ne $client.ConnectionSpecificSuffixSearchList -and @($client.ConnectionSpecificSuffixSearchList).Count -eq 1 -and $client.ConnectionSpecificSuffixSearchList[0] -ceq $Fixture.suffix) 'Adapter DNS search list readback differs'
    $interface=Get-NetIPInterface -InterfaceIndex $Receipt.interfaceIndex -AddressFamily IPv4
    Require ((Test-JsonInteger $Receipt.metric 0 ([uint32]::MaxValue)) -and $interface.InterfaceMetric -eq $Receipt.metric) 'Owned resolver interface metric readback differs'
}
function Read-DnsCounts([string]$Path) {
    $value=Read-SharedJson $Path
    foreach ($field in @('udpAnswers','tcpAnswers','bareKnownLabelQueries','expectedFqdnQueries','otherQueries')) {
        Require (Test-JsonInteger $value.$field 0 ([long]::MaxValue)) 'DNS responder receipt invalid'
    }
    $value
}
function Invoke-ResolverCheck([string]$Label,[scriptblock]$Query,$Fixture) {
    $before=Read-DnsCounts $Fixture.receiptPath
    $passed=$false;$errorType=$null
    try {$passed=[bool](&$Query)} catch {$errorType=$_.Exception.GetType().FullName}
    $after=Read-DnsCounts $Fixture.receiptPath
    $fresh=$after.udpAnswers -gt $before.udpAnswers -or $after.tcpAnswers -gt $before.tcpAnswers
    [ordered]@{check=$Label;required=($Label -ne 'cmdletShortName');passed=($passed -and $fresh);responseMatches=$passed;freshTunnelAnswer=$fresh;errorType=$errorType;before=$before;after=$after}
}
function Test-NormalResolver($Fixture) {
    $before=Read-DnsCounts $Fixture.receiptPath
    $checks=@()
    foreach ($tcp in @($false,$true)) {
        $queryName=if ($tcp) {'tcp.'+$Fixture.fqdn} else {$Fixture.fqdn}
        $parameters=@{Name=$queryName;Type='A';DnsOnly=$true;NoHostsFile=$true;QuickTimeout=$true;ErrorAction='Stop'}
        if ($tcp) {$parameters['TcpOnly']=$true}
        $label=if ($tcp) {'fqdnTcp'} else {'fqdnUdp'}
        $checks+=Invoke-ResolverCheck $label {
            $answers=@(Resolve-DnsName @parameters | Where-Object {$_.Type -eq 'A'})
            $answers.Count -eq 1 -and $answers[0].IPAddress -ceq $Fixture.answer
        } $Fixture
    }
    $checks+=Invoke-ResolverCheck 'cmdletShortName' {
        $answers=@(Resolve-DnsName -Name $Fixture.shortName -Type A -DnsOnly -NoHostsFile -QuickTimeout -ErrorAction Stop | Where-Object {$_.Type -eq 'A'})
        $answers.Count -eq 1 -and $answers[0].IPAddress -ceq $Fixture.answer -and $answers[0].Name.TrimEnd('.') -ieq $Fixture.fqdn
    } $Fixture
    $checks+=Invoke-ResolverCheck 'applicationShortName' {
        $task=[Net.Dns]::GetHostAddressesAsync($Fixture.shortName)
        Require ($task.Wait(10000)) 'Application resolver timed out'
        $answers=@($task.GetAwaiter().GetResult())
        $answers.Count -eq 1 -and $answers[0].ToString() -ceq $Fixture.answer
    } $Fixture
    $after=Read-DnsCounts $Fixture.receiptPath
    $observed=$after.udpAnswers -gt $before.udpAnswers -and $after.tcpAnswers -gt $before.tcpAnswers
    [ordered]@{passed=(@($checks | Where-Object {$_.required -and -not $_.passed}).Count -eq 0 -and $observed);tunnelUdpTcpObserved=$observed;checks=$checks}
}
function Read-ExcludedRoute {
    $routes=@(Find-NetRoute -RemoteIPAddress '10.88.0.129' -ErrorAction Stop | Where-Object {$_.DestinationPrefix})
    Require ($routes.Count -eq 1) 'Excluded destination route lookup was ambiguous'
    $routes[0] | Select-Object InterfaceIndex,DestinationPrefix,NextHop
}
function Read-FullFixtureRoute([string]$Address) {
    Require ($Address -in @('127.0.0.1','10.88.1.1','10.88.0.54','10.88.0.2','10.88.0.3')) 'Unexpected full-tunnel fixture destination'
    $routes=@(Find-NetRoute -RemoteIPAddress $Address -ErrorAction Stop | Where-Object {$_.DestinationPrefix})
    Require ($routes.Count -eq 1) 'Full-tunnel fixture route lookup was ambiguous'
    $routes[0] | Select-Object InterfaceIndex,DestinationPrefix,NextHop
}
function Assert-FullFixture($Fixture) {
    Require ($Fixture.target -ceq '10.88.0.54' -and $Fixture.port -eq 18443 -and $Fixture.peer -ceq '127.0.0.1' -and $Fixture.excludedTarget -ceq '10.88.1.1' -and $Fixture.hostInternetPreserved -is [bool] -and $Fixture.hostInternetPreserved) 'Unexpected bounded full-tunnel fixture'
}
function Get-ManyFixtureRoutes {
    if ($NestedRoutes) {
        foreach ($prefix in 27..32) {
            $size=1 -shl (32-$prefix)
            for ($offset=0;$offset -lt 256;$offset+=$size) {
                [pscustomobject]@{prefix="10.88.0.$offset/$prefix";directive="route 10.88.0.$offset 255.255.255.$(256-$size)"}
            }
        }
        return
    }
    for ($number=0;$number -lt 504;$number++) {
        $third=2+[int][Math]::Floor($number/128);$last=2*($number%128)
        [pscustomobject]@{prefix="10.88.$third.$last/32";directive="route 10.88.$third.$last 255.255.255.255"}
    }
}
function Assert-ManyFixture($Fixture) {
    $expected=@(Get-ManyFixtureRoutes | ForEach-Object {$_.prefix})
    Require ($Fixture.manyRoutesDisjoint -is [bool] -and $Fixture.manyRoutesDisjoint -eq (-not $NestedRoutes) -and (-not $NestedRoutes -or ($Fixture.manyRoutesNested -is [bool] -and $Fixture.manyRoutesNested)) -and $Fixture.manyRoutePrefixes -is [Array] -and ($Fixture.manyRoutePrefixes -join ',') -ceq ($expected -join ',')) 'Fresh 505-route fixture matching the explicit route shape required'
    if ($NestedRoutes) {Require (($Fixture.nestedRouteTargets -join ',') -ceq '10.88.0.2,10.88.0.3') 'Fixed ordinary and broadcast-endpoint traffic targets required'}
}
function Assert-ManyFixtureRoutes($Routes,[uint32]$ExpectedIndex) {
    $expected=@('10.88.0.0/24')+@(Get-ManyFixtureRoutes | ForEach-Object {$_.prefix})
    $requested=@($Routes | Where-Object {$_.DestinationPrefix -in $expected})
    $actual=@($requested.DestinationPrefix | Sort-Object -Unique)
    Require ($expected.Count -eq 505 -and $requested.Count -eq 505 -and $actual.Count -eq 505 -and @(Compare-Object $expected $actual).Count -eq 0) 'Large exact owned route set was not actually installed'
    Require ($ExpectedIndex -gt 0 -and @($requested | Where-Object {$_.InterfaceIndex -ne $ExpectedIndex -or $_.NextHop -cne '0.0.0.0' -or $_.RouteMetric -ne 5}).Count -eq 0) 'Large route interface, next hop or metric differs'
    foreach ($row in $requested) {
        $localBroadcast=$NestedRoutes -and $row.DestinationPrefix -match '^10\.88\.0\.([0-9]+)/32$' -and ([int]$Matches[1]%2 -eq 1)
        Require ([int]$row.Protocol -eq 3 -or ([int]$row.Protocol -eq 2 -and $localBroadcast)) 'Large route protocol differs from negotiated/native broadcast metadata'
    }
}
function Test-FullFixtureTraffic($Receipt,$Fixture,$PeerBefore,$ExcludedBefore) {
    Require ($Receipt.fullTunnel -is [bool] -and $Receipt.fullTunnel -and $Receipt.peerPathVerified -is [bool] -and $Receipt.peerPathVerified) 'Full-tunnel/transport path receipt missing'
    foreach ($pair in @(@{address=$Fixture.peer;before=$PeerBefore},@{address=$Fixture.excludedTarget;before=$ExcludedBefore})) {
        $selected=Read-FullFixtureRoute $pair.address
        Require ($selected.InterfaceIndex -ne $Receipt.interfaceIndex -and ($selected | ConvertTo-Json -Compress) -ceq ($pair.before | ConvertTo-Json -Compress)) 'Transport or excluded route changed'
    }
    $targets=@($Fixture.target);if ($NestedRoutes) {$targets+=@($Fixture.nestedRouteTargets)}
    foreach ($target in $targets) {
    $selected=Read-FullFixtureRoute $target
    $targetPrefix=if ($NestedRoutes) {"$target/32"} else {'10.88.0.0/24'}
    Require ($selected.InterfaceIndex -eq $Receipt.interfaceIndex -and $selected.DestinationPrefix -ceq $targetPrefix) 'Full-plan target did not select derived fixture route'
    $payload=[byte[]]::new(262144);$rng=[Security.Cryptography.RandomNumberGenerator]::Create()
    try {$rng.GetBytes($payload)} finally {$rng.Dispose()}
    $hasher=[Security.Cryptography.SHA256]::Create()
    try {$expected=$hasher.ComputeHash($payload)} finally {$hasher.Dispose()}
    $client=[Net.Sockets.TcpClient]::new()
    try {
        Require ($client.ConnectAsync($target,$Fixture.port).Wait(5000)) 'Full-plan TCP connect timed out'
        $stream=$client.GetStream();$stream.ReadTimeout=5000;$stream.WriteTimeout=5000
        $stream.Write($payload,0,$payload.Length);$client.Client.Shutdown([Net.Sockets.SocketShutdown]::Send)
        $actual=[byte[]]::new(32);$offset=0
        while ($offset -lt $actual.Length) {$read=$stream.Read($actual,$offset,$actual.Length-$offset);Require ($read -gt 0) 'Full-plan digest response truncated';$offset+=$read}
        Require ([Convert]::ToBase64String($actual) -ceq [Convert]::ToBase64String($expected)) 'Full-plan tunnel payload digest differs'
    } finally {$client.Dispose()}
    }
}
function Assert-ExclusionRoutes($Routes,$Selected,$Before,[uint32]$OwnedIndex) {
    Require ('10.88.0.0/25' -in @($Routes.DestinationPrefix) -and '10.88.0.53/32' -in @($Routes.DestinationPrefix)) 'Effective include or DNS route missing'
    Require ($Selected.InterfaceIndex -ne $OwnedIndex -and ($Selected | ConvertTo-Json -Compress) -ceq ($Before | ConvertTo-Json -Compress)) 'Excluded destination no longer uses its original route'
}
function Start-DnsClient {
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$helperPath;$start.Arguments='--validation-client';$start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $start.RedirectStandardInput=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    $client=[Diagnostics.Process]::new();$client.StartInfo=$start
    Require ($client.Start()) 'Validation client did not start'
    $client
}
if ($SelfTest) {
    $fixture=[pscustomobject]@{server='10.88.0.53';answer='10.88.0.53';suffix='0123456789abcdef.termterm.test';shortName='probe';fqdn='probe.0123456789abcdef.termterm.test';profile=[pscustomobject]@{family='openVpn';config="client`nremote 127.0.0.1 31199`n"}}
    Assert-DnsFixture $fixture
    foreach ($field in @('server','suffix','fqdn')) {
        $old=$fixture.$field;$fixture.$field='untrusted-value';$rejected=$false
        try {Assert-DnsFixture $fixture} catch {$rejected=$true}
        Require $rejected 'Invalid DNS fixture accepted';$fixture.$field=$old
    }
    function Get-NetAdapter {param([switch]$IncludeHidden) [pscustomobject]@{InterfaceGuid='11111111-1111-1111-1111-111111111111';InterfaceIndex=33}}
    function Get-DnsClientServerAddress {param($InterfaceIndex,$AddressFamily) [pscustomobject]@{ServerAddresses=@('10.88.0.53')}}
    function Get-DnsClient {param($InterfaceIndex) [pscustomobject]@{ConnectionSpecificSuffix='0123456789abcdef.termterm.test';ConnectionSpecificSuffixSearchList=@('0123456789abcdef.termterm.test')}}
    function Get-NetIPInterface {param($InterfaceIndex,$AddressFamily) [pscustomobject]@{InterfaceMetric=5}}
    $receipt=[pscustomobject]@{id='test-session';generation=1;guid='11111111-1111-1111-1111-111111111111';luid='123';interfaceIndex=33;metric=5;dns=[pscustomobject]@{servers=@('10.88.0.53');searchDomain=$fixture.suffix;searchList=@($fixture.suffix)}}
    Assert-OwnedDns $receipt $fixture 'test-session'
    Assert-OwnedDns ($receipt | ConvertTo-Json -Depth 5 | ConvertFrom-Json) $fixture 'test-session'
    $receipt.dns.searchList=@('external.test');$rejected=$false
    try {Assert-OwnedDns $receipt $fixture 'test-session'} catch {$rejected=$true}
    Require $rejected 'Changed adapter search list accepted';$receipt.dns.searchList=@($fixture.suffix)
    foreach ($field in @('id','guid','luid','interfaceIndex','metric')) {
        $old=$receipt.$field;$receipt.$field=if ($field -in @('interfaceIndex','metric')) {99} else {'invalid'};$rejected=$false
        try {Assert-OwnedDns $receipt $fixture 'test-session'} catch {$rejected=$true}
        Require $rejected 'Mismatched owned network receipt accepted';$receipt.$field=$old
    }
    $ownErrors=$null
    $ownAst=[Management.Automation.Language.Parser]::ParseFile($PSCommandPath,[ref]$null,[ref]$ownErrors)
    Require (-not $ownErrors) 'DNS runner did not parse'
    $explicitServers=@($ownAst.FindAll({param($node) $node -is [Management.Automation.Language.CommandParameterAst] -and $node.ParameterName -ceq 'Server'},$true))
    Require ($explicitServers.Count -eq 0) 'Resolver test bypasses the normal Windows server selection'
    $pending=[Threading.Tasks.TaskCompletionSource[string]]::new()
    $watch=[Diagnostics.Stopwatch]::StartNew();$rejected=$false
    try {Wait-InventoryOutput $pending.Task 50} catch {$rejected=$_.ToString().Contains('output timed out')}
    Require ($rejected -and $watch.ElapsedMilliseconds -lt 3000) 'Pending inventory output was not rejected within its deadline'
    $pending.SetResult('finished');Require ((Wait-InventoryOutput $pending.Task 50) -ceq 'finished') 'Completed inventory output was lost'
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=(Get-Process -Id $PID).Path
    $start.Arguments='-NoProfile -NonInteractive -Command "[Threading.Thread]::Sleep(15000)"'
    $start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    $probe=[Diagnostics.Process]::new();$probe.StartInfo=$start;$brokerReport=[ordered]@{}
    $watch.Restart();$rejected=$false
    try {Invoke-BoundedInventoryProbe $probe 'selfTest' 100 100} catch {$rejected=$_.ToString().Contains('native inventory timed out')}
    Require ($rejected -and $watch.ElapsedMilliseconds -lt 5000 -and -not $brokerReport.Contains('selfTestNativeCleanupFailed')) 'Hung inventory process was not rejected and cleaned within its deadline'
    $routeBefore=[pscustomobject]@{InterfaceIndex=1;DestinationPrefix='0.0.0.0/0';NextHop='192.0.2.1'}
    $routes=@([pscustomobject]@{DestinationPrefix='10.88.0.0/25'},[pscustomobject]@{DestinationPrefix='10.88.0.53/32'})
    Assert-ExclusionRoutes $routes $routeBefore $routeBefore 2
    foreach ($invalid in @(@{routes=@();selected=$routeBefore},@{routes=$routes;selected=[pscustomobject]@{InterfaceIndex=2;DestinationPrefix='10.88.0.128/25';NextHop='0.0.0.0'}})) {
        $rejected=$false
        try {Assert-ExclusionRoutes $invalid.routes $invalid.selected $routeBefore 2} catch {$rejected=$true}
        Require $rejected 'Invalid exclusion route evidence accepted'
    }
    $fullFixture=[pscustomobject]@{target='10.88.0.54';port=18443;peer='127.0.0.1';excludedTarget='10.88.1.1';hostInternetPreserved=$true}
    Assert-FullFixture $fullFixture
    $fullFixture.target='unapproved-target';$rejected=$false
    try {Assert-FullFixture $fullFixture} catch {$rejected=$true}
    Require $rejected 'Unbounded full-tunnel fixture accepted'
    foreach ($nested in @($false,$true)) {
        $NestedRoutes=$nested;$many=@(Get-ManyFixtureRoutes)
        $range=if ($nested) {'^10\.88\.0\.[0-9]+/(27|28|29|30|31|32)$'} else {'^10\.88\.[2-5]\.[0-9]+/32$'}
        Require ($many.Count -eq 504 -and @($many.prefix | Select-Object -Unique).Count -eq 504 -and @($many | Where-Object {$_.prefix -notmatch $range}).Count -eq 0) 'Large route fixture escaped its isolated prefixes or changed size'
        $manyFixture=[pscustomobject]@{manyRoutesDisjoint=(-not $nested);manyRoutesNested=$nested;manyRoutePrefixes=@($many.prefix);nestedRouteTargets=@('10.88.0.2','10.88.0.3')};Assert-ManyFixture $manyFixture
        $manyFixture.manyRoutesDisjoint=$nested;$rejected=$false;try {Assert-ManyFixture $manyFixture} catch {$rejected=$true};Require $rejected 'Mismatched route shape accepted'
        $manyRows=@('10.88.0.0/24')+@($many.prefix) | ForEach-Object {[pscustomobject]@{DestinationPrefix=$_;InterfaceIndex=33;NextHop='0.0.0.0';RouteMetric=5;Protocol=3}}
        Assert-ManyFixtureRoutes $manyRows 33
        $rejected=$false;try {Assert-ManyFixtureRoutes $manyRows[1..504] 33} catch {$rejected=$true}
        Require $rejected 'Incomplete large route set was accepted'
        $duplicate=@($manyRows[1..504])+$manyRows[1];$rejected=$false;try {Assert-ManyFixtureRoutes $duplicate 33} catch {$rejected=$true};Require $rejected 'Duplicate plus missing requested prefix accepted'
        foreach ($field in @('InterfaceIndex','NextHop','RouteMetric','Protocol')) {
            $changed=$manyRows | ConvertTo-Json | ConvertFrom-Json;$changed[0].$field=if ($field -eq 'NextHop') {'192.0.2.1'} else {99}
            $rejected=$false;try {Assert-ManyFixtureRoutes $changed 33} catch {$rejected=$true};Require $rejected 'Changed requested route attributes accepted'
        }
    }
    $integers=ConvertFrom-Json '{"index":7,"maximum":4294967295,"metric":1,"counter":0}'
    Require ((Test-JsonInteger $integers.index 1 ([uint32]::MaxValue)) -and (Test-JsonInteger $integers.maximum 1 ([uint32]::MaxValue)) -and (Test-JsonInteger $integers.metric 0 ([uint32]::MaxValue)) -and (Test-JsonInteger $integers.counter 0 ([long]::MaxValue))) 'JSON integer roundtrip rejected'
    foreach ($invalid in @('7',7.5,$true,-1,[long]4294967296)) {
        Require (-not (Test-JsonInteger $invalid 0 ([uint32]::MaxValue))) 'Invalid JSON integer accepted'
    }
    $script:selfTestDnsReads=0;$script:selfTestDnsAdvances=$true
    function Read-DnsCounts {param($Path) if ($script:selfTestDnsAdvances) {$script:selfTestDnsReads++};[pscustomobject]@{udpAnswers=$script:selfTestDnsReads;tcpAnswers=0;bareKnownLabelQueries=0;expectedFqdnQueries=0;otherQueries=0}}
    $failed=Invoke-ResolverCheck 'fixture-failure' {throw 'test-query-failed'} $fixture
    Require (-not $failed.passed -and $failed.errorType) 'Resolver failure was not retained in diagnostics'
    $succeeded=Invoke-ResolverCheck 'fixture-success' {$true} $fixture
    Require ($succeeded.passed -and -not $succeeded.errorType) 'Resolver success was lost in diagnostics'
    $script:selfTestDnsAdvances=$false
    $cached=Invoke-ResolverCheck 'fixture-no-fresh-answer' {$true} $fixture
    Require (-not $cached.passed -and $cached.responseMatches -and -not $cached.freshTunnelAnswer) 'Cached response incorrectly proves fresh tunnel traffic'
    $optional=Invoke-ResolverCheck 'cmdletShortName' {throw 'test-query-failed'} $fixture
    Require (-not $optional.required -and -not $optional.passed -and $failed.required -and $cached.required) 'Application resolver evidence was replaced by the optional cmdlet diagnostic'
    function Read-DnsBaseline {[ordered]@{fixture='state'}}
    $script:selfTestInventoryReads=0
    function Read-NativeInventory {param($Phase,$ExitTimeout,$OutputTimeout) $script:selfTestInventoryReads++;if ($script:selfTestInventoryReads -eq 1) {throw 'Native inventory did not prove no Wintun devices and no owned registered identities'};[pscustomobject]@{readOnly=$true;safe=$true;devices=@();registeredTermTermInterfaces=@()}}
    $clean=Wait-NativeCleanup (Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress)
    Require ($script:selfTestInventoryReads -eq 2 -and $clean.safe -and $clean.devices.Count -eq 0) 'Transient native cleanup was not rechecked without recovery'
    [ordered]@{selfTest=$true;passed=37;sharedFunctions=$functionNames.Count;privilegedActions=$false;networkActions=$false} | ConvertTo-Json -Compress
    return
}
Require ($Helper -and $ExpectedManifestSha256 -and $Fixtures -and $ExpectedFixturesSha256 -and $ExpectedValidatorSha256) 'Reviewed helper, manifest, fixture and shared validator hashes required'
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
Require (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) 'Run unelevated; only the fixed helper requests UAC'
$helperPath=(Resolve-Path -LiteralPath $Helper).Path
Require ([IO.Path]::GetFileName($helperPath) -ceq 'termterm-vpn-broker.exe') 'Unexpected helper filename'
$bundle=Split-Path -Parent $helperPath
Require (-not ($FullTunnel -and $RouteExclusions)) 'Use either the bounded full-tunnel or split-route fixture'
Require (-not $ManyRoutes -or $FullTunnel) 'Large route teardown requires the bounded full-tunnel fixture'
Require (-not $NestedRoutes -or $ManyRoutes) 'Nested route teardown requires explicit large-route mode'
$variant=if ($FullTunnel) {'-full-tunnel'} elseif ($RouteExclusions) {'-exclusions'} else {''}
if ($ManyRoutes) {$variant+='-many-routes'}
$reportPath=Join-Path $bundle "dns-v1-$Mode$variant-result.json"
Require (-not (Test-Path -LiteralPath $reportPath)) 'Use a fresh acceptance bundle; preserve earlier evidence'
$locks=[Collections.Generic.List[IO.FileStream]]::new()
$brokerProcess=$null;$baselineJson=$null;$bundleVerified=$false
$brokerReport=[ordered]@{ok=$false;mode=$Mode;routeExclusions=[bool]$RouteExclusions;fullTunnel=[bool]$FullTunnel;manyRoutes=[bool]$ManyRoutes;nestedRoutes=[bool]$NestedRoutes;sharedValidatorSha256=$validatorHash;events=@();cleanupEvidence='pending';companyNetworkTested=$false}
$script:brokerReplies=@{};$script:brokerStates=@{};$script:brokerSessions=@{};$script:brokerAllowedFailures=@{};$script:brokerRequestId=0;$script:brokerReadTask=$null
try {
    Require (@(Get-Process -Name 'termterm-vpn-broker' -ErrorAction SilentlyContinue).Count -eq 0) 'Another development broker is active; preserve it and stop this acceptance run'
    $manifest=(Read-LockedFile (Join-Path $bundle 'manifest.json') $ExpectedManifestSha256) | ConvertFrom-Json
    foreach ($name in @('termterm-vpn-broker.exe','termterm-openvpn.exe','wintun.dll','fixture-ca.pem')) {Require ($name -in @($manifest.files.name)) 'Required reviewed bundle member missing'}
    foreach ($file in $manifest.files) {
        Require ([IO.Path]::GetFileName($file.name) -ceq $file.name) 'Invalid manifest member'
        $null=Read-LockedFile (Join-Path $bundle $file.name) $file.sha256
    }
    $helperHash=($manifest.files | Where-Object {$_.name -ceq 'termterm-vpn-broker.exe'}).sha256
    $audit=Read-SharedJson (Join-Path $bundle 'dns-journal-audit-before.json')
    Require ($audit.ok -is [bool] -and $audit.ok -and $audit.readOnly -is [bool] -and $audit.readOnly -and $audit.recoveryPerformed -is [bool] -and -not $audit.recoveryPerformed -and $audit.phase -ceq 'before' -and $audit.helperSha256 -ceq $helperHash -and $audit.protectedJournalCount -eq 0) 'Reviewed empty-journal baseline audit required before connection'
    $fixtureData=(Read-LockedFile (Resolve-Path -LiteralPath $Fixtures).Path $ExpectedFixturesSha256) | ConvertFrom-Json
    $dns=$fixtureData.dnsFixture;Assert-DnsFixture $dns
    if ($FullTunnel) {
        Assert-FullFixture $dns.fullTunnel
        $peerBefore=Read-FullFixtureRoute $dns.fullTunnel.peer
        $fullExcludedBefore=Read-FullFixtureRoute $dns.fullTunnel.excludedTarget
    } else {Require (-not $dns.fullTunnel) 'Full-tunnel fixture requires explicit FullTunnel mode'}
    if ($ManyRoutes) {Assert-ManyFixture $dns.fullTunnel;$dns.profile.config+="`n"+((Get-ManyFixtureRoutes).directive -join "`n")+"`n"}
    if ($RouteExclusions) {
        $dns.profile.config+="`nroute 10.88.0.0 255.255.255.0`nroute 10.88.0.128 255.255.255.128 net_gateway`n"
        $excludedBefore=Read-ExcludedRoute
    }
    Require ([IO.Path]::GetFullPath($dns.receiptPath) -ceq (Join-Path (Split-Path -Parent (Resolve-Path -LiteralPath $Fixtures).Path) 'dns-receipt.json')) 'DNS receipt must belong to the pinned fixture directory'
    $bundleVerified=$true
    $brokerReport['beforeNativeInventory']=Read-NativeInventory 'before'
    $baselineJson=Read-DnsBaseline | ConvertTo-Json -Depth 10 -Compress
    $baselinePath=Join-Path (Split-Path -Parent (Resolve-Path -LiteralPath $Fixtures).Path) "dns-baseline-$(Split-Path -Leaf $bundle)-$Mode$variant.json"
    Require (-not (Test-Path -LiteralPath $baselinePath)) 'Preserve previous private baseline; use a fresh fixture or mode'
    [IO.File]::WriteAllText($baselinePath,$baselineJson)
    $brokerReport['privateBaselineCaptured']=$true
    $brokerProcess=Start-DnsClient
    $stderrTask=$brokerProcess.StandardError.ReadToEndAsync();$brokerDeadline=[DateTime]::UtcNow.AddSeconds(120)
    $null=Wait-BrokerReply (Send-BrokerRequest @{action='hello';vaultId=[Guid]::NewGuid().ToString()})
    $id=[Guid]::NewGuid().ToString();$script:brokerSessions[$id]=@{fixture=$dns;generation=1}
    $null=Wait-BrokerReply (Send-BrokerRequest @{action='connect';id=$id;generation=1;profileId=[Guid]::NewGuid().ToString();profile=$dns.profile})
    Wait-AllConnected
    $receipt=Wait-BrokerReply (Send-BrokerRequest @{action='networkReceipt';id=$id;generation=1})
    Require ($receipt.ok -is [bool] -and $receipt.ok) 'Owned network receipt unavailable'
    Assert-OwnedDns $receipt.result $dns $id
    if ($ManyRoutes) {
        $routes=@(Get-NetRoute -InterfaceIndex $receipt.result.interfaceIndex -AddressFamily IPv4)
        Assert-ManyFixtureRoutes $routes $receipt.result.interfaceIndex
        $brokerReport['largeOwnedRouteSetVerified']=$true;$brokerReport['largeOwnedRouteCount']=505
        $expected=@('10.88.0.0/24')+@((Get-ManyFixtureRoutes).prefix)
        $brokerReport['largeOwnedRouteReadback']=@($routes | Where-Object {$_.DestinationPrefix -in $expected} | Select-Object DestinationPrefix,InterfaceIndex,NextHop,RouteMetric,Protocol)
    }
    $brokerReport['adapterDnsAndSuffixReadback']=$true
    if ($FullTunnel) {
        Test-FullFixtureTraffic $receipt.result $dns.fullTunnel $peerBefore $fullExcludedBefore
        $brokerReport['originalFullPlanAndDerivedRouteTraffic']=$true
        if ($NestedRoutes) {$brokerReport['nestedRouteTrafficTargets']=@($dns.fullTunnel.nestedRouteTargets)}
        $brokerReport['transportPeerAndExcludedRoutePreserved']=$true
        $brokerReport['hostInternetEgressTested']=$false
    }
    if ($RouteExclusions) {
        $routes=@(Get-NetRoute -InterfaceIndex $receipt.result.interfaceIndex -AddressFamily IPv4)
        Assert-ExclusionRoutes $routes (Read-ExcludedRoute) $excludedBefore $receipt.result.interfaceIndex
        $brokerReport['effectiveRouteExclusionReadback']=$true
        $brokerReport['excludedDestinationTrafficSent']=$false
    }
    $resolver=Test-NormalResolver $dns;$brokerReport['resolverBeforeConflict']=$resolver
    $resolverPassed=$resolver.passed
    $brokerReport['normalWindowsResolverUdpTcpAndSuffix']=$resolverPassed
    # The second DNS owner must be refused while the first remains usable.
    $other=[Guid]::NewGuid().ToString();$script:brokerSessions[$other]=@{fixture=$dns;generation=1}
    $null=Wait-BrokerReply (Send-BrokerRequest @{action='connect';id=$other;generation=1;profileId=[Guid]::NewGuid().ToString();profile=$dns.profile})
    $rejected=$false
    try {while ($script:brokerStates[$other] -ne 'failed') {Receive-BrokerMessage}}
    catch {
        $reason=if ($FullTunnel) {'competing_full_tunnel_session'} else {'competing_dns_session'}
        if ($_.ToString().Contains('VPN session failed: '+$reason)) {$rejected=$true} else {throw}
    }
    Require $rejected 'Competing DNS/full-tunnel owner was not rejected by admission'
    $resolver=Test-NormalResolver $dns;$brokerReport['resolverAfterConflict']=$resolver
    $resolverPassed=$resolverPassed -and $resolver.passed
    $brokerReport['competingDnsRejectedWithoutDisruption']=$resolver.passed
    $brokerReport['competingDnsRejected']=$true
    if ($FullTunnel) {$brokerReport['competingFullTunnelRejected']=$true}
    if ($FullTunnel) {
        Test-FullFixtureTraffic $receipt.result $dns.fullTunnel $peerBefore $fullExcludedBefore
        $brokerReport['fullPlanTrafficAfterConflict']=$true
    }
    $cleanupClock=[Diagnostics.Stopwatch]::StartNew()
    if ($Mode -eq 'client-death') {
        $brokerProcess.Kill();Require ($brokerProcess.WaitForExit(10000)) 'Retained client did not exit'
        $brokerReport['cleanupEvidence']='pending-owner-death'
    } else {
        $brokerDeadline=[DateTime]::UtcNow.AddSeconds($(if ($ManyRoutes) {185} else {20}))
        $brokerReport['cleanupPriorFailures']=@()
        Wait-BrokerCleanup
        $brokerProcess.StandardInput.Close();Require ($brokerProcess.WaitForExit(10000)) 'Normal client exit timed out'
        Require ($brokerProcess.ExitCode -eq 0) 'Normal client failed'
    }
    $brokerReport['afterNativeInventory']=Wait-NativeCleanup $baselineJson
    $brokerReport['cleanupElapsedMs']=$cleanupClock.ElapsedMilliseconds
    $brokerReport['baselineRestored']=$true
    if ($Mode -eq 'client-death') {
        $brokerReport['cleanupEvidence']='owner-death-baseline-and-native-inventory'
        $brokerReport['protectedJournalCleanupVerified']=$false
        $brokerReport['requiresSeparateElevatedJournalAudit']=$true
    }
    $brokerReport['ok']=$resolverPassed
} finally {
    if ($brokerProcess) {
        try {
            if (-not $brokerProcess.HasExited) {
                # Only terminate the exact retained client; its broker/watchdog owns cleanup.
                $brokerProcess.Kill();Require ($brokerProcess.WaitForExit(10000)) 'Retained client cleanup timed out'
            }
        } catch {$brokerReport['retainedClientCleanupFailed']=$true;$brokerReport['ok']=$false}
        finally {try {$brokerProcess.Dispose()} catch {$brokerReport['retainedClientDisposeFailed']=$true;$brokerReport['ok']=$false}}
    }
    if ($bundleVerified -and $baselineJson) {
        try {$brokerReport['finalBaselineRestored']=(Wait-DnsBaseline $baselineJson)} catch {$brokerReport['finalBaselineRestored']=$false}
        if (-not $brokerReport.finalBaselineRestored) {$brokerReport['ok']=$false}
    }
    foreach ($stream in $locks) {try {$stream.Dispose()} catch {$brokerReport['inputLockDisposeFailed']=$true;$brokerReport['ok']=$false}}
    [IO.File]::WriteAllText($reportPath,($brokerReport | ConvertTo-Json -Depth 12))
    Write-Output "DNS acceptance report: $reportPath"
}
Require $brokerReport.ok 'Native DNS acceptance has failed resolver checks; see redacted report'
