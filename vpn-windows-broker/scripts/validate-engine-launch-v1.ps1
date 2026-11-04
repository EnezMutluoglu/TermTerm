param(
    [Parameter(Mandatory=$true)][string]$Helper,
    [Parameter(Mandatory=$true)][string]$ExpectedHelperSha256,
    [Parameter(Mandatory=$true)][uint32]$ManagerPid,
    [Parameter(Mandatory=$true)][uint64]$ManagerBirth
)
$ErrorActionPreference='Stop'
$workspace=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
$security=[Security.AccessControl.DirectorySecurity]::new()
$security.SetAccessRuleProtection($true,$false)
$security.SetOwner($identity.User)
foreach ($sid in @($identity.User.Value,'S-1-5-18','S-1-5-32-544') | Select-Object -Unique) {
    $security.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow))
}
$output=Join-Path $workspace ('.tools/vpn-windows/engine-launch-v1-result-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff')+'-'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($output,$security) | Out-Null
$report=[ordered]@{ok=$false;output=$output;scenario='authenticated manager parent; control pipes; raw packet echo; excluded event; job close';createdAdapters=0;hostNetworkApisRequested=$false}
$image=$null;$process=$null
try {
    $principal=[Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {throw 'Explicit elevation is required for this no-network launch proof'}
    $path=(Resolve-Path -LiteralPath $Helper).Path
    if ([IO.Path]::GetFileName($path) -ne 'termterm-vpn-broker.exe') {throw 'Unexpected fixed helper name'}
    $image=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    $sha=[Security.Cryptography.SHA256]::Create()
    try {$hash=([BitConverter]::ToString($sha.ComputeHash($image))).Replace('-','').ToLowerInvariant()} finally {$sha.Dispose()}
    if ($hash -ne $ExpectedHelperSha256.ToLowerInvariant()) {throw 'Approved helper hash changed'}
    $report.helperSha256=$hash
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$path;$start.Arguments="--engine-launch-self-test --parent $ManagerPid --birth $ManagerBirth"
    $start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    $process=[Diagnostics.Process]::new();$process.StartInfo=$start
    if (-not $process.Start()) {throw 'Launch proof helper did not start'}
    $report.helperPid=$process.Id
    $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(20000)) {
        $process.Kill();$process.WaitForExit();$report.timedOut=$true
    }
    $out=$stdout.GetAwaiter().GetResult();$err=$stderr.GetAwaiter().GetResult()
    [IO.File]::WriteAllText((Join-Path $output 'helper-stdout.log'),$out)
    [IO.File]::WriteAllText((Join-Path $output 'helper-stderr.log'),$err)
    $report.helperExitCode=$process.ExitCode;$report.helperStderr=$err
    if ($report.Contains('timedOut')) {throw 'No-network launch proof timed out; helper killed, child jobs closed'}
    if ($process.ExitCode -ne 0) {throw "No-network launch proof failed with exit $($process.ExitCode)"}
    $native=$out | ConvertFrom-Json
    if ($null -eq $native -or $native.ok -isnot [bool] -or -not $native.ok -or $native.readOnlyHostNetwork -isnot [bool] -or -not $native.readOnlyHostNetwork -or $native.createdAdapters -ne 0 -or $native.unelevatedOwnerIdentityVerified -ne $true -or $native.explicitHandleListVerified -ne $true -or $native.jobBeforeResumeVerified -ne $true -or $native.probes -isnot [Array] -or $native.probes.Count -ne 2 -or $native.probes[0].scenario -ne 'echo' -or $native.probes[1].scenario -ne 'job-close' -or $native.probes[0].ok -ne $true -or $native.probes[1].ok -ne $true) {throw 'Native launch proof receipt is incomplete'}
    $report.native=$native;$report.ok=$true
} catch {$report.error=$_.Exception.Message;$report.errorDetail=$_ | Out-String}
finally {
    if ($process) {$process.Dispose()}
    if ($image) {$image.Dispose()}
    [IO.File]::WriteAllText((Join-Path $output 'engine-launch-result.json'),($report | ConvertTo-Json -Depth 12))
}
$report | ConvertTo-Json -Depth 12
if (-not $report.ok) {exit 1}
