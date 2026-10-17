# Seals resources/build-identity.json from the staged native runtime binaries.
# build-windows-app.ps1 runs it when staging, and the SignPath release
# (tooling/release-gpui/windows.ps1, package-pack) runs it again after putting
# the signed binaries in place, because signing changes their hashes.
param(
    [Parameter(Mandatory = $true)][string]$AppDir
)
$ErrorActionPreference = "Stop"

# CDXC:PlatformSupport 2026-09-14 WHY:
# Native gxserver survives app exit. Seal its runtime identity so an updated app replaces an older control plane while preserving session hosts.
$NativeHashes = @("gxserver.exe", "ghostex.exe", "wmx.exe") | ForEach-Object {
    (Get-FileHash (Join-Path (Join-Path $AppDir "resources/native") $_) -Algorithm SHA256).Hash.ToLowerInvariant()
}
$NativeHasher = [Security.Cryptography.SHA256]::Create()
try {
    $NativeFingerprint = "sha256:" + (-join ($NativeHasher.ComputeHash([Text.Encoding]::UTF8.GetBytes(($NativeHashes -join "`n") + "`n")) | ForEach-Object { $_.ToString("x2") }))
} finally { $NativeHasher.Dispose() }
$NativeIdentity = @{
    buildIdentity = "gxserver:0.1.0:$NativeFingerprint"
    fingerprint = $NativeFingerprint
    packageVersion = "0.1.0"
} | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $AppDir "resources/build-identity.json"), $NativeIdentity + "`n", [Text.UTF8Encoding]::new($false))
