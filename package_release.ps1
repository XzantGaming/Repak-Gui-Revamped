Param(
    [string]$Version = "local",
    [switch]$IncludePdb = $true,
    [switch]$SkipClean
)

$ErrorActionPreference = "Stop"

function Info($msg) { Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Warn($msg) { Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Fail($msg) { Write-Host "[FAIL] $msg" -ForegroundColor Red; exit 1 }

# Project root = this script directory
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $root

try {
    Info "Packaging repak-gui version: $Version"

    if (-not $SkipClean) {
        Info "cargo clean"
        cargo clean | Out-Null
    }

    Info "cargo build -p repak-gui --release"
    cargo build -p repak-gui --release | Out-Null

    $distDir = Join-Path $root "dist"
    $outDir  = Join-Path $distDir "repak-gui-$Version"

    if (Test-Path $outDir) { Remove-Item $outDir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null

    # Required binary
    $exePath = Join-Path $root "target/release/repak-gui.exe"
    if (-not (Test-Path $exePath)) { Fail "Binary not found: $exePath" }
    Copy-Item $exePath $outDir -Force

    # Optional PDB (symbols)
    $pdbPath = Join-Path $root "target/release/repak-gui.pdb"
    if ($IncludePdb -and (Test-Path $pdbPath)) {
        Info "Including symbols: repak-gui.pdb"
        Copy-Item $pdbPath $outDir -Force
    }

    # Optional UAssetBridge (if present in target layout)
    $bridgeDll = Join-Path $root "target/release/uassetbridge/UAssetBridge.dll"
    if (Test-Path $bridgeDll) {
        Info "Including UAssetBridge.dll"
        New-Item -ItemType Directory -Force -Path (Join-Path $outDir "uassetbridge") | Out-Null
        Copy-Item $bridgeDll (Join-Path $outDir "uassetbridge/") -Force
    } else {
        Warn "UAssetBridge.dll not found; texture post-processing will log warnings and be skipped"
    }

    # Docs
    foreach ($doc in @("README.md","CHANGELOG.md","LICENSE","RELEASE_NOTES.md")) {
        $src = Join-Path $root $doc
        if (Test-Path $src) {
            Copy-Item $src $outDir -Force
        }
    }

    # Create ZIP
    $zipPath = Join-Path $distDir ("repak-gui-" + $Version + ".zip")
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Info "Compress-Archive to $zipPath"
    Compress-Archive -Path (Join-Path $outDir "*") -DestinationPath $zipPath

    Info "Done. Upload this file to GitHub Releases: $zipPath"

    # Summary
    Write-Host ""; Write-Host "Package contents:" -ForegroundColor Green
    Get-ChildItem $outDir -Recurse | ForEach-Object { $_.FullName }

} catch {
    Fail $_
} finally {
    Pop-Location
}
