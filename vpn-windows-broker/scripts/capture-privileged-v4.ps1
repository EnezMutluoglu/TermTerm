param([switch]$RunNormal, [switch]$SelfTest)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$security = [Security.AccessControl.DirectorySecurity]::new()
$security.SetAccessRuleProtection($true,$false)
$security.SetOwner($identity.User)
foreach ($sid in @($identity.User.Value,'S-1-5-18','S-1-5-32-544') | Select-Object -Unique) {
    $rule = [Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)
    $security.AddAccessRule($rule)
}
$output = Join-Path $workspace ('.tools/vpn-windows/v4-diagnostic-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff') + '-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($output,$security) | Out-Null
$report = [ordered]@{readOnly=(-not $RunNormal);selfTest=[bool]$SelfTest;ok=$false;stage='created';output=$output}
$locks = @()

function Open-PinnedFile([string]$Path,[string]$Expected) {
    $stream = [IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try {
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $hash = ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
        if ($hash -ne $Expected) { throw "Approved file hash changed: $Path" }
        return $stream
    } catch { $stream.Dispose(); throw }
}

function Invoke-CapturedPowerShell([string]$Script,[string]$Arguments) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
    $start.Arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $Script + '" ' + $Arguments
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    $stdout = [IO.File]::Open((Join-Path $output 'runner-stdout.log'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    $stderr = [IO.File]::Open((Join-Path $output 'runner-stderr.log'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    try {
        if (-not $process.Start()) { throw 'Captured PowerShell did not start' }
        $report['childPid'] = $process.Id
        $outCopy = $process.StandardOutput.BaseStream.CopyToAsync($stdout)
        $errCopy = $process.StandardError.BaseStream.CopyToAsync($stderr)
        if (-not $process.WaitForExit(90000)) { throw 'Runner still running after 90 seconds; no process was terminated. Inspect childPid before any retry.' }
        $outCopy.GetAwaiter().GetResult() | Out-Null
        $errCopy.GetAwaiter().GetResult() | Out-Null
        $report['runnerExitCode'] = $process.ExitCode
        return $process.ExitCode
    } finally { $stdout.Dispose(); $stderr.Dispose(); $process.Dispose() }
}

try {
    if ($SelfTest) {
        if ($RunNormal) { throw 'SelfTest cannot run the normal test' }
        $report['stage'] = 'harmless-capture-self-test'
        $probe = Join-Path $output 'capture-probe.ps1'
        [IO.File]::WriteAllText($probe,"[Console]::Out.WriteLine('capture stdout'); [Console]::Error.WriteLine('capture stderr'); exit 37")
        if ((Invoke-CapturedPowerShell $probe '') -ne 37) { throw 'Wrong self-test exit code' }
        if ([IO.File]::ReadAllText((Join-Path $output 'runner-stdout.log')).Trim() -ne 'capture stdout') { throw 'Stdout capture failed' }
        if ([IO.File]::ReadAllText((Join-Path $output 'runner-stderr.log')).Trim() -ne 'capture stderr') { throw 'Stderr capture failed' }
    } else {
        $report['stage'] = 'require-elevation'
        $principal = [Security.Principal.WindowsPrincipal]::new($identity)
        if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Explicit elevation is required; this diagnostic never requests UAC itself.' }
        $report['stage'] = 'read-protected-journals'
        $root = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'TermTermVpnDevBroker-v1'
        $report['journalRootExists'] = Test-Path -LiteralPath $root
        $report['journals'] = @()
        if ($report.journalRootExists) {
            $item = Get-Item -LiteralPath $root -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Journal root is a reparse point' }
            $report['journalRootSddl'] = (Get-Acl -LiteralPath $root).Sddl
            if ($report.journalRootSddl -ne 'O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)') { throw 'Journal root protection differs; refusing contents' }
            $files = @(Get-ChildItem -LiteralPath $root -Force)
            if ($files.Count -gt 16) { throw 'Unexpected protected file count; refusing unbounded diagnostics' }
            foreach ($file in $files) {
                $entry = [ordered]@{name=$file.Name;bytes=$file.Length;attributes="$($file.Attributes)";sddl=(Get-Acl -LiteralPath $file.FullName).Sddl}
                $report.journals += $entry
                if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $file.Length -gt 2097152) { throw 'Unsafe or oversized protected file; refusing contents' }
                if ($entry.sddl -ne 'O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)' -or $file.Name -notmatch '^[0-9a-fA-F-]{36}-[0-9]+\.(json|new)$') { throw 'Unexpected protected file identity; refusing contents' }
                $stream = [IO.File]::Open($file.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
                try {
                    if ($stream.Length -gt 2097152) { throw 'Protected file grew beyond size limit' }
                    $sha = [Security.Cryptography.SHA256]::Create()
                    try { $entry['sha256'] = ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
                    $stream.Position = 0
                    $copy = [IO.File]::Open((Join-Path $output ('journal-' + $file.Name)),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
                    try { $stream.CopyTo($copy) } finally { $copy.Dispose() }
                } finally { $stream.Dispose() }
                try { $entry['journal'] = [IO.File]::ReadAllText((Join-Path $output ('journal-' + $file.Name))) | ConvertFrom-Json } catch { $entry['parseError'] = $_.ToString() }
            }
        }
        $report['stage'] = 'read-host-state'
        $report['adapters'] = @(Get-NetAdapter -IncludeHidden | Select-Object Name,InterfaceGuid,InterfaceIndex,@{Name='NetLuid';Expression={"$($_.NetLuid)"}},Status)
        $report['addresses'] = @(Get-NetIPAddress | Select-Object InterfaceIndex,AddressFamily,IPAddress,PrefixLength,PrefixOrigin,SuffixOrigin,SkipAsSource)
        $report['routes'] = @(Get-NetRoute | Select-Object InterfaceIndex,AddressFamily,DestinationPrefix,NextHop,RouteMetric,Protocol,Publish,CompartmentId)
        $report['interfaces'] = @(Get-NetIPInterface | Select-Object InterfaceIndex,AddressFamily,NlMtu,Dhcp,RouterDiscovery,Forwarding)
        $report['brokerProcesses'] = @(Get-CimInstance Win32_Process -Filter "Name='termterm-vpn-broker.exe'" | Select-Object ProcessId,ParentProcessId,CreationDate,ExecutablePath)
        $report['wintunDrivers'] = @(Get-CimInstance Win32_SystemDriver -Filter "Name='Wintun'" | Select-Object Name,State,StartMode)
        $report['comparisonNote'] = 'Compare journal GUID and decimal LUID to adapters, then interface index/address/prefix/route/MTU. This snapshot never authorizes deletion; existing Rust ownership guards remain required.'
        if ($RunNormal) {
            $report['stage'] = 'lock-approved-files'
            $runner = Join-Path $PSScriptRoot 'validate-privileged-v3.ps1'
            $helper = Join-Path $workspace '.tools/vpn-windows/broker-validation-v4/termterm-vpn-broker.exe'
            $locks += Open-PinnedFile $runner 'ed7aa11bd344ed06f3a6b9299c18dc7e6bf7ccee81157e230be7a97a403bab77'
            $locks += Open-PinnedFile $helper 'f5bbfbc514f1faf0c1e8a74ba774662c008ae1525cbcdfdcf15bc1e71a824e24'
            $locks += Open-PinnedFile (Join-Path (Split-Path -Parent $helper) 'wintun.dll') 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce'
            if ($report.brokerProcesses.Count) { throw 'Existing broker process; refusing another normal test' }
            if (@($report.journals | Where-Object {$_.name -like '*.new'}).Count) { throw 'Pending protected intent exists; refusing normal test' }
            # Existing .json journals are deliberately left for the unchanged runner's preflight rejection.
            $report['stage'] = 'capture-frozen-normal-runner'
            $exit = Invoke-CapturedPowerShell $runner ('-Helper "' + $helper + '" -ExpectedHelperSha256 f5bbfbc514f1faf0c1e8a74ba774662c008ae1525cbcdfdcf15bc1e71a824e24 -Mode normal')
            if ($exit -ne 0) { throw "Frozen runner exit $exit; exact output retained in runner-stdout.log and runner-stderr.log" }
        }
    }
    $report['ok'] = $true
    $report['stage'] = 'complete'
} catch {
    $report['error'] = $_.ToString()
    $report['exception'] = $_.Exception.ToString()
    $report['position'] = $_.InvocationInfo.PositionMessage
    $report['scriptStack'] = $_.ScriptStackTrace
} finally {
    foreach ($lock in $locks) { $lock.Dispose() }
    [IO.File]::WriteAllText((Join-Path $output 'diagnostic.json'),($report | ConvertTo-Json -Depth 20))
    Write-Output $output
}
if (-not $report.ok) { exit 1 }
