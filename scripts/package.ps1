param([switch]$SkipBuild)
$ErrorActionPreference='Stop'
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
$project=Split-Path $PSScriptRoot -Parent
Set-Location $project
Remove-Item Env:VITE_E2E -ErrorAction SilentlyContinue
if (!$SkipBuild) {
  pnpm tauri build
  if ($LASTEXITCODE -ne 0) { throw 'Windows release build failed' }
}
$version=(Get-Content -LiteralPath (Join-Path $project 'package.json') -Raw | ConvertFrom-Json).version
$release=Join-Path $project "artifacts/release-$version"
$portable=Join-Path $release 'TermTerm-win-x64'
New-Item -ItemType Directory -Force -Path $portable | Out-Null
Copy-Item -LiteralPath (Join-Path $project 'src-tauri/target/release/termterm.exe') -Destination (Join-Path $portable 'TermTerm.exe') -Force
$runtime=Join-Path $project 'src-tauri/resources/mosh'
$runtimeDestination=Join-Path $portable 'mosh'
New-Item -ItemType Directory -Force -Path $runtimeDestination | Out-Null
Get-ChildItem -LiteralPath $runtime -Force | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $runtimeDestination -Recurse -Force }
Copy-Item -LiteralPath (Join-Path $project 'docs/USER_GUIDE_TR.md') -Destination $portable -Force
Copy-Item -LiteralPath (Join-Path $project 'docs/STATUS.md') -Destination $portable -Force
Copy-Item -LiteralPath (Join-Path $project 'docs/THIRD_PARTY.md') -Destination $portable -Force
$licenseDestination=Join-Path $portable 'licenses'
New-Item -ItemType Directory -Force -Path $licenseDestination | Out-Null
Get-ChildItem -LiteralPath (Join-Path $project 'docs/licenses') -File | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $licenseDestination -Force }
Get-ChildItem -LiteralPath (Join-Path $project 'src-tauri/target/release/bundle/nsis') -Filter "*_${version}_*.exe" | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $release -Force }
Get-ChildItem -LiteralPath (Join-Path $project 'src-tauri/target/release/bundle/nsis') -Filter "*_${version}_*.exe.sig" | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $release -Force }
# Include the same offline WebView2 prerequisite for fresh Windows machines.
$webviewCandidates=@(Get-ChildItem -LiteralPath (Join-Path $env:LOCALAPPDATA 'tauri') -Filter '*WebView2*X64*.exe' -Recurse -File -ErrorAction SilentlyContinue)
if (!$webviewCandidates.Count) { $webviewCandidates=@(Get-ChildItem -LiteralPath (Join-Path $env:LOCALAPPDATA 'tauri') -Filter '*WebView2*.exe' -Recurse -File -ErrorAction SilentlyContinue | Where-Object {$_.Length -gt 50MB}) }
if (!$webviewCandidates.Count) { throw 'Offline WebView2 runtime missing; build the configured NSIS installer first.' }
$prerequisites=Join-Path $portable 'prerequisites'
New-Item -ItemType Directory -Force -Path $prerequisites | Out-Null
Copy-Item -LiteralPath ($webviewCandidates | Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName -Destination (Join-Path $prerequisites 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe') -Force
@'
Extract this entire folder and start TermTerm.exe.
If WebView2 is unavailable, run prerequisites/MicrosoftEdgeWebView2RuntimeInstallerX64.exe once.
Keep Mosh helpers and licenses beside the application.
PostgreSQL is optional. Create a local encrypted vault to start without a server.
'@ | Set-Content -LiteralPath (Join-Path $portable 'START_HERE.txt') -Encoding utf8
Compress-Archive -LiteralPath $portable -DestinationPath (Join-Path $release "TermTerm-$version-windows-x64-portable.zip") -Force

# Explicit source allowlist: no test credentials, private vaults, node_modules or build products.
Add-Type -AssemblyName System.IO.Compression
$sourceArchive=Join-Path $release "TermTerm-$version-source.zip"
$sourceStream=[System.IO.File]::Open($sourceArchive,[System.IO.FileMode]::Create)
$zip=[System.IO.Compression.ZipArchive]::new($sourceStream,[System.IO.Compression.ZipArchiveMode]::Create)
try {
  $folders=@('src','src-tauri/src','src-tauri/capabilities','src-tauri/icons','migrations','scripts','templates','docs','tests','.github')
  $files=@('README.md','.gitignore','package.json','pnpm-lock.yaml','pnpm-workspace.yaml','index.html','app-icon.svg','tsconfig.json','vite.config.ts','playwright.config.ts','wdio.conf.mjs','vitest.config.ts','src-tauri/Cargo.toml','src-tauri/Cargo.lock','src-tauri/build.rs','src-tauri/tauri.conf.json','src-tauri/tauri.e2e.conf.json','src-tauri/tauri.windows.conf.json','src-tauri/tauri.macos.conf.json','src-tauri/tauri.linux.conf.json')
  $files+=@('rust-toolchain.toml','.stignore-shared','AGENTS.md','src-tauri/tauri.release.conf.json')
  foreach($folder in $folders){$files+=Get-ChildItem -LiteralPath (Join-Path $project $folder) -File -Recurse | ForEach-Object {$_.FullName.Substring($project.Length+1)}}
  foreach($file in $files){$path=Join-Path $project $file;$entry=$zip.CreateEntry(($file -replace '\\','/'));$input=[System.IO.File]::OpenRead($path);$output=$entry.Open();try{$input.CopyTo($output)}finally{$input.Dispose();$output.Dispose()}}
} finally { $zip.Dispose();$sourceStream.Dispose() }
Get-ChildItem -LiteralPath $release -File | Where-Object {$_.Name -ne 'SHA256SUMS.txt'} | Get-FileHash -Algorithm SHA256 | ForEach-Object { "$($_.Hash.ToLower())  $([System.IO.Path]::GetFileName($_.Path))" } | Set-Content -LiteralPath (Join-Path $release 'SHA256SUMS.txt')
Get-ChildItem -LiteralPath $release -File | Select-Object Name,Length
$global:LASTEXITCODE=0
