$ErrorActionPreference='Stop'
$taskDirectory=Join-Path $PSScriptRoot 'wintun-close-probe-20261103-v1/exit-code-selftest'
if (Test-Path -LiteralPath $taskDirectory) {throw 'Preserve previous exit-code self-test'}
New-Item -ItemType Directory -Path $taskDirectory | Out-Null
$taskResults=@()
foreach($taskRetainHandle in @($false,$true)) {
    foreach($taskExpected in @(0,17)) {
        $taskPrefix=Join-Path $taskDirectory (([string]$taskRetainHandle)+'-'+$taskExpected)
        $taskProcess=Start-Process -FilePath (Join-Path $env:windir 'System32/cmd.exe') -ArgumentList @('/d','/c','exit',"$taskExpected") -WindowStyle Hidden -PassThru -RedirectStandardOutput ($taskPrefix+'.stdout.log') -RedirectStandardError ($taskPrefix+'.stderr.log')
        try {
            if($taskRetainHandle){$taskOwnedHandle=$taskProcess.Handle}
            if(-not $taskProcess.WaitForExit(5000)) {$taskProcess.Kill();throw 'Inert child did not exit'}
            $taskProcess.WaitForExit()
            $taskActual=$taskProcess.ExitCode
            $taskResults+=@([ordered]@{retainedHandle=$taskRetainHandle;expected=$taskExpected;actual=$taskActual;matched=$null -ne $taskActual -and $taskActual -eq $taskExpected})
        } finally {$taskProcess.Dispose()}
    }
}
$taskResults | ConvertTo-Json -Compress
if (@($taskResults | Where-Object {$_.retainedHandle -and -not $_.matched}).Count) {throw 'Pinned native process handle did not preserve the actual exit code'}
