param(
  [Parameter(Mandatory=$true)][string]$Output,
  [Parameter(Mandatory=$true)][string]$Runtime,
  [Parameter(Mandatory=$true)][string]$WebViewRuntime,
  [Parameter(Mandatory=$true)][string]$WebViewSha256,
  [string]$BuildCache,
  [switch]$ReuseBuildCache,
  [ValidateSet('0.4.5')][string]$Candidate,
  [switch]$ReleaseCandidate
)
$ErrorActionPreference = 'Stop'
if ($ReuseBuildCache -and !$BuildCache) { throw 'Reusing a build cache requires its explicit path.' }
if ($ReleaseCandidate -and !$Candidate) { throw 'A release candidate requires the explicit -Candidate 0.4.5 contract.' }
$project = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $project
$outputPath = if ([IO.Path]::IsPathRooted($Output)) { [IO.Path]::GetFullPath($Output) } else { [IO.Path]::GetFullPath((Join-Path $project $Output)) }
$tools = [IO.Path]::GetFullPath((Join-Path $project '.tools/vpn-windows')) + [IO.Path]::DirectorySeparatorChar
if (!$outputPath.StartsWith($tools, [StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $outputPath)) {
  throw 'Use a new output directory inside this worktree .tools; previous packages are preserved.'
}
$runtimePath = (Resolve-Path -LiteralPath $Runtime).Path
if (!$runtimePath.StartsWith($tools, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use the locally verified dependency collection inside .tools.' }
$preflightText = python scripts/collect-vpn-runtime.py --package-preflight --runtime $runtimePath --output $outputPath
if ($LASTEXITCODE -ne 0) { throw 'VPN package preflight failed before any build or output creation.' }
$preflight = ($preflightText -join "`n") | ConvertFrom-Json
$configPath = if ($Candidate) { 'src-tauri/tauri.vpn-candidate.conf.json' } else { 'src-tauri/tauri.vpn-dev.conf.json' }
$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$windows = Get-Content -LiteralPath 'src-tauri/tauri.vpn-windows-package.conf.json' -Raw | ConvertFrom-Json
if ($Candidate) {
  if ($config.identifier -ne 'local.termterm.desktop' -or $config.productName -ne 'TermTerm' -or $config.version -ne $Candidate -or $config.mainBinaryName -ne 'termterm' -or $config.build.features.Count -ne 1 -or $config.build.features[0] -ne 'vpn-candidate') { throw 'Exact production candidate build contract required.' }
  $windows = $config
} elseif ($config.identifier -ne 'local.termterm.desktop.vpndev' -or $config.productName -ne 'TermTerm VPN Dev') {
  throw 'Isolated VPN development identity is required.'
}
if ($config.bundle.active -or $config.bundle.createUpdaterArtifacts -or $config.plugins.updater.endpoints.Count) {
  throw 'Isolated VPN development identity and disabled updater are required.'
}
$desktopName = $windows.mainBinaryName + '.exe'
if ($desktopName -ne $(if ($Candidate) {'termterm.exe'} else {'termterm-vpn-dev.exe'})) { throw 'Windows executable differs from the selected build contract.' }
$configs = @('--config', $configPath)
$updaterEndpoints = @()
if (!$Candidate) { $configs += @('--config', 'src-tauri/tauri.vpn-windows-package.conf.json') }
if ($ReleaseCandidate) {
  $releasePath = 'src-tauri/tauri.vpn-release-candidate.conf.json'
  $release = Get-Content -LiteralPath $releasePath -Raw | ConvertFrom-Json
  $stable = Get-Content -LiteralPath 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
  if ($release.version -ne $Candidate -or $release.identifier -ne $config.identifier -or $release.mainBinaryName -ne $config.mainBinaryName -or
      ($release.plugins.updater.endpoints -join '`n') -ne ($stable.plugins.updater.endpoints -join '`n') -or $release.plugins.updater.pubkey -or $release.bundle.createUpdaterArtifacts) {
    throw 'Release candidate must preserve the existing signed stable updater contract.'
  }
  $configs += @('--config', $releasePath)
  $updaterEndpoints = @($release.plugins.updater.endpoints)
}
$candidateArgs = if ($Candidate) { @('--candidate', $Candidate) } else { @() }
$brokerFeatures = if ($Candidate) { @('--features', 'production-candidate') } else { @() }
$manifestName = if ($Candidate) { 'vpn-bundle.json' } else { 'vpn-dev-bundle.json' }
$packageName = if ($ReleaseCandidate) { 'TermTerm-Release-Candidate' } elseif ($Candidate) { 'TermTerm-Candidate' } else { 'TermTerm-VPN-Dev' }
$webview = (Resolve-Path -LiteralPath $WebViewRuntime).Path
if ($WebViewSha256 -notmatch '^[a-f0-9]{64}$' -or (Get-FileHash -LiteralPath $webview -Algorithm SHA256).Hash.ToLower() -ne $WebViewSha256) { throw 'Offline WebView2 prerequisite hash mismatch.' }
$signature = Get-AuthenticodeSignature -LiteralPath $webview
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw 'Offline WebView2 requires a valid Microsoft publisher signature.' }
function Source-Receipt {
  $receipt = python scripts/collect-vpn-runtime.py --source-receipt
  if ($LASTEXITCODE -ne 0) { throw 'Public source receipt failed.' }
  return ($receipt -join "`n")
}
$before = Source-Receipt
New-Item -ItemType Directory -Path $outputPath | Out-Null
$dependencyStage = Join-Path $outputPath 'dependency-source'
foreach ($relative in $preflight.runtimeFiles) {
  $destination = Join-Path $dependencyStage $relative
  New-Item -ItemType Directory -Force -Path (Split-Path $destination -Parent) | Out-Null
  Copy-Item -LiteralPath (Join-Path $runtimePath $relative) -Destination $destination
}
$target = Join-Path $outputPath 'target'
$brokerTarget = Join-Path $outputPath 'broker-target'
if ($BuildCache) {
  $cache = (Resolve-Path -LiteralPath $BuildCache).Path
  if (!$cache.StartsWith($tools, [StringComparison]::OrdinalIgnoreCase)) { throw 'Reuse only this VPN worktree development build cache.' }
  $ancestor = Get-Item -LiteralPath $cache
  while ($ancestor) {
    if ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Build cache ancestors cannot be reparse points.' }
    $ancestor = $ancestor.Parent
  }
  foreach ($directory in @('target','broker-target')) {
    $from = Join-Path $cache $directory
    $to = Join-Path $outputPath $directory
    if ((Get-Item -LiteralPath $from -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Build cache roots cannot be reparse points.' }
    if (Get-ChildItem -LiteralPath $from -Recurse -Force | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw 'Build cache cannot contain reparse points.' }
    if ($ReuseBuildCache) {
      if ($directory -eq 'target') { $target = $from } else { $brokerTarget = $from }
    } else {
      robocopy.exe $from $to /E /COPY:DAT /R:0 /W:0 /NFL /NDL /NJH /NJS /XD bundle nsis wix | Out-Null
      if ($LASTEXITCODE -ge 8) { throw 'Development build cache copy failed.' }
    }
  }
}
$oldTarget = $env:CARGO_TARGET_DIR
$oldChannel = $env:TERMTERM_RELEASE_CHANNEL
$oldE2e = $env:VITE_E2E
$oldPath = $env:PATH
try {
  $env:CARGO_TARGET_DIR = $target
  $env:TERMTERM_RELEASE_CHANNEL = if ($ReleaseCandidate) { 'stable' } elseif ($Candidate) { 'candidate' } else { 'development' }
  Remove-Item Env:VITE_E2E -ErrorAction SilentlyContinue
  $env:PATH = "$env:USERPROFILE/.cargo/bin;$env:PATH"
  cargo build --locked --release --manifest-path vpn-windows-broker/Cargo.toml --target-dir $brokerTarget @brokerFeatures
  if ($LASTEXITCODE -ne 0) { throw 'Default broker build failed.' }
  pnpm.cmd exec tauri build --no-bundle @configs
  if ($LASTEXITCODE -ne 0) { throw 'Isolated VPN desktop build failed.' }
  $binary = Join-Path $target ('release/' + $desktopName)
  $versionInfo = (Get-Item -LiteralPath $binary).VersionInfo
  if ($versionInfo.ProductName -ne $config.productName -or $versionInfo.ProductVersion -ne $config.version) { throw 'Built executable has the wrong identity/version.' }
  $assets = if ($Candidate) { 'dist-vpn-candidate/assets' } else { 'dist-vpn-dev/assets' }
  if (Get-ChildItem -LiteralPath $assets -Filter '*.js' | Select-String -Pattern '__nativeEvents|__E2E__') { throw 'Frontend test hook found.' }
  $stage = Join-Path $outputPath 'desktop'
  python scripts/stage-vpn-windows-dev.py --application $binary --broker (Join-Path $brokerTarget 'release/termterm-vpn-broker.exe') --destination $stage @candidateArgs
  if ($LASTEXITCODE -ne 0) { throw 'Pinned VPN desktop staging failed.' }
  $nsisStage = Join-Path $outputPath 'installer-desktop'
  python scripts/stage-vpn-windows-dev.py --application $binary --broker (Join-Path $brokerTarget 'release/termterm-vpn-broker.exe') --destination $nsisStage --bundle-type nsis @candidateArgs
  if ($LASTEXITCODE -ne 0) { throw 'NSIS-specific desktop staging failed.' }
  $freshText = python scripts/collect-vpn-runtime.py --package-preflight --runtime $runtimePath
  if ($LASTEXITCODE -ne 0 -or ($freshText -join "`n") -ne ($preflightText -join "`n")) { throw 'Verified package inputs changed during the build.' }
  foreach ($asset in $preflight.assets) {
    foreach ($directory in @($stage,$nsisStage)) {
      $destination = Join-Path $directory $asset.target
      New-Item -ItemType Directory -Force -Path (Split-Path $destination -Parent) | Out-Null
      Copy-Item -LiteralPath $asset.source -Destination $destination
    }
  }
  $prerequisites = Join-Path $stage 'prerequisites'
  New-Item -ItemType Directory -Path $prerequisites | Out-Null
  Copy-Item -LiteralPath $webview -Destination (Join-Path $prerequisites 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe')
  @'
TermTerm VPN Dev — local development package
Extract the complete folder and open termterm-vpn-dev.exe.
If WebView2 is missing, install the Microsoft-signed offline runtime in prerequisites first.
Keep all helpers, DLLs, licenses and notices beside the application.
This separate development identity does not use the stable update feed.
VPN connections require the separate Windows UAC flow; native tests and installation are on hold.
'@ | Set-Content -LiteralPath (Join-Path $stage 'START_HERE.txt') -Encoding utf8
  if ($Candidate) {
    @"
TermTerm production candidate $Candidate — local evaluation only; not an approved release.
Extract the complete folder and open termterm.exe. Keep every helper, DLL and notice beside it.
This uses the production application identity. Evaluate only in a disposable machine; never install over the protected stable app.
The updater is disabled. VPN elevation pairs the exact same-directory desktop/broker bundle and current unelevated Windows logon.
Native package acceptance remains required. The installer has not been run by this packaging script.
"@ | Set-Content -LiteralPath (Join-Path $stage 'START_HERE.txt') -Encoding utf8
    if ($ReleaseCandidate) {
      $readme = Join-Path $stage 'START_HERE.txt'
      (Get-Content -LiteralPath $readme -Raw).Replace('The updater is disabled.', 'Optional updates use the existing owner-controlled feed and existing signature verification. This local candidate has not been published; publication requires separate approval of these exact bytes.') | Set-Content -LiteralPath $readme -Encoding utf8
    }
  }
  $resources = @{}
  Get-ChildItem -LiteralPath $nsisStage | Where-Object { $_.Name -ne $desktopName } | ForEach-Object { $resources[$_.FullName.Replace('\','/')] = $_.Name + $(if ($_.PSIsContainer) {'/'} else {''}) }
  $overlay = @{
    identifier = $config.identifier
    productName = $config.productName
    version = $config.version
    mainBinaryName = $windows.mainBinaryName
    bundle = @{ active=$true; targets=@('nsis'); createUpdaterArtifacts=$false; resources=$resources; windows=@{ nsis=@{ installMode='currentUser'; displayLanguageSelector=$false; startMenuFolder=$config.productName } } }
    plugins = @{ updater=@{ endpoints=$updaterEndpoints } }
  }
  $overlayPath = Join-Path $outputPath 'tauri.vpn-package.conf.json'
  $overlay | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $overlayPath -Encoding utf8
  pnpm.cmd exec tauri bundle --ci --no-sign --bundles nsis @configs --config $overlayPath
  if ($LASTEXITCODE -ne 0) { throw 'Isolated VPN installer generation failed.' }
  if ($before -ne (Source-Receipt)) { throw 'Source changed during packaging; retain this output for diagnosis, do not accept it.' }
  $manifest = Get-Content -LiteralPath (Join-Path $stage $manifestName) -Raw | ConvertFrom-Json
  if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLower() -ne $manifest.files.$desktopName) { throw 'Desktop changed after staging.' }
  $installers = @(Get-ChildItem -LiteralPath (Join-Path $target 'release/bundle/nsis') -Filter '*.exe')
  if ($installers.Count -ne 1) { throw 'Expected exactly one isolated development installer.' }
  python scripts/stage-vpn-windows-dev.py --verify-installer $installers[0].FullName --destination $nsisStage @candidateArgs
  if ($LASTEXITCODE -ne 0) { throw 'Actual installer payload hash verification failed.' }
  Copy-Item -LiteralPath $installers[0].FullName -Destination $outputPath
  Compress-Archive -LiteralPath $stage -DestinationPath (Join-Path $outputPath ($packageName + '-' + $config.version + "-windows-x64-portable.zip"))
  Compress-Archive -LiteralPath $dependencyStage -DestinationPath (Join-Path $outputPath ($packageName + '-' + $config.version + "-dependency-source.zip"))
  $before | Set-Content -LiteralPath (Join-Path $outputPath 'SOURCE-RECEIPT.json') -Encoding utf8
  Get-ChildItem -LiteralPath $outputPath -File | Where-Object { $_.Extension -in @('.exe','.zip') } | ForEach-Object {
    (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower() + '  ' + $_.Name
  } | Set-Content -LiteralPath (Join-Path $outputPath 'SHA256SUMS.txt') -Encoding utf8
  Write-Output ('Prepared local ' + $config.version + ' development packages only. Installer was not run; distributionReady remains false.')
} finally {
  $env:CARGO_TARGET_DIR = $oldTarget
  $env:TERMTERM_RELEASE_CHANNEL = $oldChannel
  $env:VITE_E2E = $oldE2e
  $env:PATH = $oldPath
}
