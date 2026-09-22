param([ValidateSet('setup','start','stop','status','backup')][string]$Action='status')
$ErrorActionPreference='Stop'
$project = Split-Path $PSScriptRoot -Parent
$output = Join-Path $project '.lab'
New-Item -ItemType Directory -Force -Path $output | Out-Null
$who = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
icacls $output /inheritance:r /grant:r "${who}:(OI)(CI)F" 'SYSTEM:(OI)(CI)F' | Out-Null
if ($Action -in @('setup','start')) {
  $marker = Join-Path $output 'keepalive.pid'
  $existing = if (Test-Path -LiteralPath $marker) { Get-CimInstance Win32_Process -Filter "ProcessId=$(Get-Content -LiteralPath $marker)" -ErrorAction SilentlyContinue } else { $null }
  if (!$existing -or $existing.CommandLine -notlike '*termterm*lab-keepalive.sh*') {
    $linuxKeepalive = '/mnt/' + $project.Substring(0,1).ToLowerInvariant() + ($project.Substring(2) -replace '\\','/') + '/scripts/lab-keepalive.sh'
    $process = Start-Process wsl.exe -WindowStyle Hidden -ArgumentList @('-d','Ubuntu-24.04','-u','root','--exec','bash',$linuxKeepalive) -PassThru
    $process.Id | Set-Content -LiteralPath $marker
  }
}
$linuxScript = (wsl -d Ubuntu-24.04 -- wslpath -u ((Join-Path $PSScriptRoot 'lab.sh') -replace '\\','/')).Trim()
$linuxOutput = (wsl -d Ubuntu-24.04 -- wslpath -u ($output -replace '\\','/')).Trim()
wsl -d Ubuntu-24.04 -u root -- bash $linuxScript $Action $linuxOutput
if ($LASTEXITCODE -ne 0) { throw "Lab operation failed: $Action" }
if ($Action -eq 'stop') {
  $marker = Join-Path $output 'keepalive.pid'
  if (Test-Path -LiteralPath $marker) {
    $existing = Get-CimInstance Win32_Process -Filter "ProcessId=$(Get-Content -LiteralPath $marker)" -ErrorAction SilentlyContinue
    if ($existing -and $existing.CommandLine -like '*termterm*lab-keepalive.sh*') { Stop-Process -Id $existing.ProcessId }
  }
}
