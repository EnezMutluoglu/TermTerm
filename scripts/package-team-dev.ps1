param()
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $project
$version = (Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version
if ($version -notmatch '-dev\.') { throw 'This script only packages a development version.' }
$config = Get-Content -LiteralPath 'src-tauri/tauri.team-dev.conf.json' -Raw | ConvertFrom-Json
if ($config.identifier -ne 'local.termterm.desktop.teamdev' -or $config.bundle.active -or $config.plugins.updater.endpoints.Count) {
  throw 'Development identity/update isolation is required.'
}
$env:PATH = "$env:USERPROFILE/.cargo/bin;$env:PATH"
$env:TERMTERM_RELEASE_CHANNEL = 'development'
Remove-Item Env:VITE_E2E -ErrorAction SilentlyContinue
pnpm.cmd team:build
if ($LASTEXITCODE -ne 0) { throw 'Development application build failed.' }
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $project 'src-tauri/target' }
$binary = Join-Path $target 'debug/termterm.exe'
if (!(Test-Path -LiteralPath $binary)) { throw 'Development executable was not produced.' }
$dependencies = cargo tree --manifest-path src-tauri/Cargo.toml --edges normal --prefix none
if ($LASTEXITCODE -ne 0 -or ($dependencies -match 'tauri-plugin-wdio')) { throw 'A test driver must not be in the development package.' }
if (Get-ChildItem -LiteralPath 'dist/assets' -Filter '*.js' | Select-String -Pattern '__nativeEvents|__E2E__') {
  throw 'A frontend test hook is present in the development bundle.'
}
$output = Join-Path $project "artifacts/team-$version"
$portable = Join-Path $output 'TermTerm-Team-Dev-windows-x64'
New-Item -ItemType Directory -Force -Path $portable | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $portable 'TermTerm-Team-Dev.exe') -Force
foreach ($pair in @(@('src-tauri/resources/mosh', 'mosh'), @('docs/licenses', 'licenses'))) {
  $source = Join-Path $project $pair[0]
  $destination = Join-Path $portable $pair[1]
  New-Item -ItemType Directory -Force -Path $destination | Out-Null
  Get-ChildItem -LiteralPath $source -Force | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $destination -Recurse -Force }
}
foreach ($file in @('TEAM_GUIDE_TR.md', 'VALIDATION_TEAM_0.4.0-dev.1.md', 'THIRD_PARTY.md')) {
  Copy-Item -LiteralPath (Join-Path $project "docs/$file") -Destination $portable -Force
}
$webview = Get-ChildItem -LiteralPath (Join-Path $env:LOCALAPPDATA 'tauri') -Filter '*WebView2*X64*.exe' -Recurse -File |
  Where-Object { $_.Length -gt 50MB } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (!$webview) { throw 'Cached offline WebView2 prerequisite is missing. No download was started.' }
$prerequisites = Join-Path $portable 'prerequisites'
New-Item -ItemType Directory -Force -Path $prerequisites | Out-Null
Copy-Item -LiteralPath $webview.FullName -Destination (Join-Path $prerequisites 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe') -Force
@'
TermTerm Team — geliştirme denemesi

Klasörün tamamını çıkarın, TermTerm-Team-Dev.exe dosyasını açın.
WebView2 bulunmayan Windows'ta prerequisites içindeki çevrimdışı kurucuyu çalıştırın.
Mosh ve licenses klasörlerini executable yanında tutun.

Team hesabı için PostgreSQL, TLS CA ve şirket profili gereklidir.
Bu bilgisayardaki özel test giriş bilgileri kaynak projenin .lab/TEAM_DEMO_LOCAL.txt dosyasındadır.
Bu paket test hesabı, özel anahtar, PostgreSQL parolası veya kullanıcı kasası içermez.
Kişisel kasa sunucusuz açılabilir. Team için önce çevrimiçi hesap girişi gerekir.

Ayrı uygulama kimliği: local.termterm.desktop.teamdev
Kurulu kararlı uygulamanın kasalarını kullanmaz. Otomatik güncelleme yayını yoktur.
İmzasız geliştirme executable'ıdır; üretim/kararlı sürüm değildir.
Kullanım için TEAM_GUIDE_TR.md, sonuçlar için VALIDATION_TEAM_0.4.0-dev.1.md okuyun.
'@ | Set-Content -LiteralPath (Join-Path $portable 'BASLANGIC.txt') -Encoding utf8
$zip = Join-Path $output "TermTerm-Team-$version-windows-x64-portable.zip"
Compress-Archive -LiteralPath $portable -DestinationPath $zip -Force
Get-FileHash -LiteralPath $zip, (Join-Path $portable 'TermTerm-Team-Dev.exe') -Algorithm SHA256 |
  ForEach-Object { "$($_.Hash.ToLower())  $([IO.Path]::GetRelativePath($output, $_.Path).Replace('\','/'))" } |
  Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding utf8
Get-Item -LiteralPath $zip | Select-Object Name, Length
$global:LASTEXITCODE = 0
