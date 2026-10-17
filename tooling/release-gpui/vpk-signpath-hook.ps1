# CDXC:Release 2026-10-17 WHY:
# The `vpk pack --signTemplate` command for SignPath releases (windows.ps1).
# Velopack runs it once per file it would sign: every PE file of the pack
# directory, including the Update.exe (Squirrel.exe) and launcher stub it
# generates, and finally its Setup.exe. SignPath can only sign from a GitHub
# Actions step, so the release packs twice:
#   collect  copy the file to $GHOSTEX_VPK_SIGN_DIR/<sha256>/<name> for SignPath
#            round 1, leaving the pack's own copy unsigned.
#   replace  windows.ps1 has already put the signed copies of the app's own
#            binaries into the pack directory, so a file that carries a
#            signature is kept. Any other file (the Update.exe and stub Velopack
#            generates) is overwritten with its signed copy from
#            $GHOSTEX_VPK_SIGN_DIR/<sha256>/<name>, keyed by the SHA-256 of the
#            unsigned bytes, so a file whose bytes differ from the collected pass
#            fails the release instead of shipping unsigned.
# The Setup.exe is skipped in both modes: it embeds the full package and is
# signed in SignPath round 2, after the real pack.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$File
)
$ErrorActionPreference = "Stop"

$Mode = $env:GHOSTEX_VPK_SIGN_MODE
$SignDir = $env:GHOSTEX_VPK_SIGN_DIR
if ($Mode -notin @("collect", "replace") -or -not $SignDir) {
    throw "GHOSTEX_VPK_SIGN_MODE (collect or replace) and GHOSTEX_VPK_SIGN_DIR are required"
}
$Name = Split-Path -Leaf $File
if ($Name -like "*-Setup.exe") {
    Write-Host "Leaving $Name for the Setup.exe SignPath round"
    exit 0
}
if ($Mode -eq "replace" -and (Get-AuthenticodeSignature -LiteralPath $File).SignerCertificate) {
    Write-Host "Keeping the signed $Name"
    exit 0
}
$Sha = (Get-FileHash -Algorithm SHA256 -LiteralPath $File).Hash.ToLowerInvariant()
$Entry = Join-Path (Join-Path $SignDir $Sha) $Name

if ($Mode -eq "collect") {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Entry) | Out-Null
    Copy-Item -LiteralPath $File -Destination $Entry -Force
    Write-Host "Collected $Name ($Sha)"
    exit 0
}

if (-not (Test-Path -LiteralPath $Entry)) {
    throw "No SignPath-signed copy of $Name ($Sha); its bytes differ from the collected pass"
}
if (-not (Get-AuthenticodeSignature -LiteralPath $Entry).SignerCertificate) {
    throw "The SignPath copy of $Name carries no Authenticode signature"
}
Copy-Item -LiteralPath $Entry -Destination $File -Force
Write-Host "Replaced $Name with its SignPath-signed copy"
