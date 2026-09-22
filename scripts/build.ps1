param([ValidateSet('check','test','dev','package')][string]$Action='package')
$ErrorActionPreference='Stop'
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
$project = Split-Path $PSScriptRoot -Parent
Set-Location $project
switch ($Action) {
  'check' { cargo check --manifest-path src-tauri/Cargo.toml; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; pnpm build }
  'test' { cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; pnpm test:unit; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; pnpm test:ui }
  'dev' { pnpm desktop }
  'package' { & (Join-Path $PSScriptRoot 'package.ps1') }
}
exit $LASTEXITCODE
