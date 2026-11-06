param(
    [Parameter(Mandatory=$true)][string]$Helper,
    [Parameter(Mandatory=$true)][string]$ExpectedHelperSha256,
    [Parameter(Mandatory=$true)][ValidateSet('before','after-normal','after-client-death')][string]$Phase
)
$ErrorActionPreference='Stop'
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {throw 'Explicit elevation required for read-only protected-journal audit'}
$helperPath=(Resolve-Path -LiteralPath $Helper).Path
$toolsRoot=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../.tools/vpn-windows'))+[IO.Path]::DirectorySeparatorChar
if (-not $helperPath.StartsWith($toolsRoot,[StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($helperPath) -cne 'termterm-vpn-broker.exe') {throw 'Use an isolated reviewed VPN test helper'}
if ((Get-FileHash -LiteralPath $helperPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExpectedHelperSha256) {throw 'Reviewed helper hash changed'}
$output=Join-Path (Split-Path -Parent $helperPath) "dns-journal-audit-$Phase.json"
if (Test-Path -LiteralPath $output) {throw 'Preserve previous audit; use a fresh test bundle'}
$journalRoot=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'TermTermVpnDevBroker-v1'
$report=[ordered]@{ok=$false;phase=$Phase;helperSha256=$ExpectedHelperSha256;readOnly=$true;recoveryPerformed=$false;utc=[DateTime]::UtcNow.ToString('o')}
try {
    $count=0
    if (Test-Path -LiteralPath $journalRoot) {
        if ((Get-Item -LiteralPath $journalRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {throw 'Protected root is a reparse point'}
        $count=@(Get-ChildItem -LiteralPath $journalRoot -Filter '*.json' -Force -ErrorAction Stop).Count
    }
    $report['protectedJournalCount']=$count
    if ($count -ne 0) {throw 'Protected journals remain; no recovery attempted'}
    $report['ok']=$true
} finally {
    [IO.File]::WriteAllText($output,($report | ConvertTo-Json -Depth 3))
    Write-Output "Read-only journal audit report: $output"
}
