param([string]$ReleasePath)
$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$version=(Get-Content -LiteralPath (Join-Path $project 'package.json') -Raw | ConvertFrom-Json).version
if (!$ReleasePath) { $ReleasePath=Join-Path $project "artifacts/release-$version" }
$release=(Resolve-Path -LiteralPath $ReleasePath).Path
Add-Type -AssemblyName System.IO.Compression.FileSystem
foreach($line in Get-Content -LiteralPath (Join-Path $release 'SHA256SUMS.txt')) {
  if ($line -notmatch '^([0-9a-f]{64})  ([^/\\]+)$') { throw 'Invalid checksum manifest' }
  $expected=$Matches[1]; $name=$Matches[2]
  $actual=(Get-FileHash -LiteralPath (Join-Path $release $name) -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "Checksum mismatch: $name" }
}
$exe=Join-Path $release 'TermTerm-win-x64/TermTerm.exe'
$bytes=[System.IO.File]::ReadAllBytes($exe)
if ($bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) { throw 'Not a Windows executable' }
$pe=[BitConverter]::ToInt32($bytes,0x3c)
if ([BitConverter]::ToUInt16($bytes,$pe+4) -ne 0x8664) { throw 'Executable is not x64' }
$source=[System.IO.Compression.ZipFile]::OpenRead((Join-Path $release "TermTerm-$version-source.zip"))
try {
  $names=@($source.Entries | ForEach-Object {$_.FullName -replace '\\','/'})
  if ($names | Where-Object {($_ -match '(^|/)(\.lab|\.tools|node_modules|target)/|\.(ttvault|ttbackup|pem|key)$|connection\.json|ssh\.json|(^|/)\.\./') -and $_ -notmatch '^tests/ssh-keys/(rsa4096\.aes(128|192|256)|ecdsa(256|384|521)\.aes256)\.pem$'}) { throw 'Source archive includes a private or build path' }
  foreach($required in @('README.md','rust-toolchain.toml','src-tauri/Cargo.lock','docs/USER_GUIDE_TR.md','docs/STATUS.md','docs/VALIDATION_0.3.md','docs/POSTGRES_FLOW.md','docs/BUILD_PLATFORMS.md','docs/licenses/THIRD_PARTY_LICENSES.txt','migrations/001_sync.sql','scripts/lab.ps1','.github/workflows/desktop.yml')) {
    if ($required -notin $names) { throw "Missing source entry: $required" }
  }
} finally { $source.Dispose() }
$portable=[System.IO.Compression.ZipFile]::OpenRead((Join-Path $release "TermTerm-$version-windows-x64-portable.zip"))
try {
  $names=@($portable.Entries | ForEach-Object {$_.FullName -replace '\\','/'})
  foreach($required in @('TermTerm.exe','prerequisites/MicrosoftEdgeWebView2RuntimeInstallerX64.exe','mosh/bin/mosh-client.exe','mosh/bin/cygwin1.dll','mosh/usr/share/doc/mosh/COPYING','licenses/THIRD_PARTY_LICENSES.txt','USER_GUIDE_TR.md','STATUS.md')) {
    if ('TermTerm-win-x64/'+$required -notin $names) { throw "Missing portable entry: $required" }
  }
} finally { $portable.Dispose() }
Write-Output 'Verified: SHA-256 manifests, Windows x64 executable, source allowlist, portable Mosh runtime, notices and guides.'
