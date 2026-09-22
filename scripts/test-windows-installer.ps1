param([string]$Installer)
$ErrorActionPreference='Stop'
$project=(Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$version=(Get-Content -LiteralPath (Join-Path $project 'package.json') -Raw | ConvertFrom-Json).version
if (!$Installer) { $Installer=Join-Path $project "src-tauri/target/release/bundle/nsis/TermTerm_${version}_x64-setup.exe" }
$target=[IO.Path]::GetFullPath((Join-Path $project '.tools/installer-check-0.3'))
if (!$target.StartsWith($project+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Installer test target must remain inside the workspace.' }
$registered=@(Get-ChildItem -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' | Where-Object {$_.PSChildName -match '^TermTerm$|^local\.termterm\.desktop$'})
if ($registered.Count) { throw 'An existing TermTerm installation is registered. Preserve it; use a clean Windows account for the installer smoke test.' }
$running=@(Get-Process termterm -ErrorAction SilentlyContinue)
if ($running.Count) { throw 'TermTerm is running, possibly as a portable application. Preserve its sessions; run the installer smoke test in a clean Windows account after closing it yourself.' }
if (Test-Path -LiteralPath $target) { throw 'Installer test directory already exists; inspect it before reusing.' }
$shortcuts=@((Join-Path ([Environment]::GetFolderPath('Desktop')) 'TermTerm.lnk'),(Join-Path ([Environment]::GetFolderPath('Programs')) 'TermTerm.lnk'))
$saved=@{}
foreach($file in $shortcuts) { if(Test-Path -LiteralPath $file) {$saved[$file]=[IO.File]::ReadAllBytes($file)} }
try {
  $process=Start-Process -FilePath $Installer -ArgumentList @('/S',"/D=$target") -WindowStyle Hidden -Wait -PassThru
  if($process.ExitCode -ne 0){throw "Installer exited with $($process.ExitCode)"}
  $exe=Join-Path $target 'termterm.exe'
  if(!(Test-Path -LiteralPath $exe)){throw 'Installed executable is missing.'}
  # Tauri changes only this fixed-width bundle marker while creating NSIS.
  $expectedBytes=[IO.File]::ReadAllBytes((Join-Path $project 'src-tauri/target/release/termterm.exe'))
  $installedBytes=[IO.File]::ReadAllBytes($exe)
  $marker=[Text.Encoding]::ASCII.GetBytes('__TAURI_BUNDLE_TYPE_VAR_UNK')
  $offset=[Text.Encoding]::ASCII.GetString($expectedBytes).IndexOf('__TAURI_BUNDLE_TYPE_VAR_UNK',[StringComparison]::Ordinal)
  if($offset -lt 0){throw 'Release binary has no expected Tauri bundle marker.'}
  if([Text.Encoding]::ASCII.GetString($installedBytes,$offset,$marker.Length) -ne '__TAURI_BUNDLE_TYPE_VAR_NSS'){throw 'Installed binary has an unexpected Tauri bundle marker.'}
  [Array]::Copy($marker,0,$installedBytes,$offset,$marker.Length)
  $sha=[Security.Cryptography.SHA256]::Create()
  try {
    if([Convert]::ToBase64String($sha.ComputeHash($installedBytes)) -ne [Convert]::ToBase64String($sha.ComputeHash($expectedBytes))){throw 'Installed executable differs from release binary beyond the Tauri bundle marker.'}
  } finally {$sha.Dispose()}
  $helper=Join-Path $target 'mosh/bin/mosh-client.exe'
  if(!(Test-Path -LiteralPath $helper)){throw 'Installed Mosh runtime is missing.'}
  & $helper --version
  if($LASTEXITCODE -ne 0){throw 'Installed Mosh runtime cannot start.'}
  $app=Start-Process -FilePath $exe -WindowStyle Hidden -PassThru
  try {
    $deadline=[DateTime]::UtcNow.AddSeconds(25)
    do {Start-Sleep -Milliseconds 250;$app.Refresh()} while (!$app.HasExited -and !$app.MainWindowHandle -and [DateTime]::UtcNow -lt $deadline)
    if($app.HasExited -or !$app.MainWindowHandle){throw 'Installed application did not open its window.'}
    if(Get-NetTCPConnection -State Listen -OwningProcess $app.Id -ErrorAction SilentlyContinue | Where-Object {$_.LocalPort -eq 4445}){throw 'Production application exposed a test driver.'}
    [void]$app.CloseMainWindow()
    if(!$app.WaitForExit(15000)){throw 'Installed application did not close gracefully.'}
  } finally {if(!$app.HasExited){Stop-Process -Id $app.Id -Force}}
  $uninstaller=Get-ChildItem -LiteralPath $target -File -Filter '*uninstall*.exe' | Select-Object -First 1
  if(!$uninstaller){throw 'Uninstaller is missing.'}
  $remove=Start-Process -FilePath $uninstaller.FullName -ArgumentList @('/S',"_?=$target") -WindowStyle Hidden -Wait -PassThru
  if($remove.ExitCode -ne 0){throw "Uninstaller exited with $($remove.ExitCode)"}
  if(Test-Path -LiteralPath $exe){throw 'Uninstaller left the application binary behind.'}
  # NSIS cannot delete its own running image with _?=; remove only that known file.
  Remove-Item -LiteralPath $uninstaller.FullName
  if (!(Get-ChildItem -LiteralPath $target -Force)) { Remove-Item -LiteralPath $target }
  Write-Output 'Verified silent installation, installed executable hash, bundled Mosh, real window, graceful close, no production test port and uninstall.'
} finally {
  foreach($file in $saved.Keys){[IO.File]::WriteAllBytes($file,$saved[$file])}
}
