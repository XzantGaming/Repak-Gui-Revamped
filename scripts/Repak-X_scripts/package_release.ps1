param(
    [string]$Configuration = "release",
    [switch]$Zip
)

$ErrorActionPreference = "Stop"

function Invoke-CargoBuild {
    param([string]$Config)
    Write-Host "Building cargo ($Config)..."
    $cargoArgs = @("build", "--$Config")
    $proc = Start-Process -FilePath "cargo" -ArgumentList $cargoArgs -NoNewWindow -PassThru -Wait
    if ($proc.ExitCode -ne 0) { throw "cargo build failed with exit code $($proc.ExitCode)" }
}

function Get-WorkspaceRoot {
    # Scripts are in scripts/Repak-X_scripts/, so go up 2 levels to repo root
    $scriptDir = Split-Path -Parent $PSCommandPath
    return Split-Path -Parent (Split-Path -Parent $scriptDir)
}

function Get-Version {
    param([string]$CargoTomlPath)
    $content = Get-Content -Path $CargoTomlPath -Raw
    $m = [regex]::Match($content, '(?m)^version\s*=\s*"([^"]+)"')
    if ($m.Success) { return $m.Groups[1].Value }
    return "0.0.0"
}

$root = Get-WorkspaceRoot
$targetDir = Join-Path $root "target"
$profileDir = Join-Path $targetDir $Configuration
$exePath = Join-Path $profileDir "REPAK-X.exe"

# Build the project (this will also auto-publish UAssetBridge via build.rs changes)
Push-Location $root
Invoke-CargoBuild -Config $Configuration
Pop-Location

# Verify outputs
if (!(Test-Path $exePath)) { throw "REPAK-X.exe not found at $exePath" }

# Determine app version from workspace Cargo.toml (or fallback to repak-gui/Cargo.toml)
$cargoRoot = Join-Path $root "Cargo.toml"
$cargoGui = Join-Path $root "repak-x\Cargo.toml"
$version = if (Test-Path $cargoRoot) { Get-Version -CargoTomlPath $cargoRoot } elseif (Test-Path $cargoGui) { Get-Version -CargoTomlPath $cargoGui } else { "0.0.0" }

# Create dist folder
$distRoot = Join-Path $root "dist"
$appFolderName = "Repak-X-v$version"
$distDir = Join-Path $distRoot $appFolderName

Write-Host "Creating dist at $distDir"
New-Item -ItemType Directory -Force -Path $distDir | Out-Null

# Copy main binary
Copy-Item -LiteralPath $exePath -Destination (Join-Path $distDir "REPAK-X.exe") -Force


# Build and copy AssetTypeCli
Write-Host "Building AssetTypeCli..." -ForegroundColor Cyan
$assetTypeCliDir = Join-Path $root "tools\AssetTypeCli"
if (Test-Path $assetTypeCliDir) {
    $assetTypeCliDist = Join-Path $distRoot "AssetTypeCli"
    & dotnet publish "$assetTypeCliDir\AssetTypeCli.csproj" -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -o $assetTypeCliDist
    if ($LASTEXITCODE -eq 0) {
        $destAssetTypeCli = Join-Path $distDir "AssetTypeCli"
        New-Item -ItemType Directory -Force -Path $destAssetTypeCli | Out-Null
        Copy-Item -Path (Join-Path $assetTypeCliDist "AssetTypeCli.exe") -Destination $destAssetTypeCli -Force
        Write-Host "AssetTypeCli built successfully" -ForegroundColor Green
    }
    else {
        Write-Warning "AssetTypeCli build failed"
    }
}

# Build and copy ExportMapCli
Write-Host "Building ExportMapCli..." -ForegroundColor Cyan
$exportMapCliDir = Join-Path $root "tools\ExportMapCli"
if (Test-Path $exportMapCliDir) {
    $exportMapCliDist = Join-Path $distRoot "ExportMapCli"
    & dotnet publish "$exportMapCliDir\ExportMapCli.csproj" -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -o $exportMapCliDist
    if ($LASTEXITCODE -eq 0) {
        $destExportMapCli = Join-Path $distDir "ExportMapCli"
        New-Item -ItemType Directory -Force -Path $destExportMapCli | Out-Null
        Copy-Item -Path (Join-Path $exportMapCliDist "ExportMapCli.exe") -Destination $destExportMapCli -Force
        Write-Host "ExportMapCli built successfully" -ForegroundColor Green
    }
    else {
        Write-Warning "ExportMapCli build failed"
    }
}

# Oodle DLL is now downloaded on-demand by the app
# No need to bundle it - this avoids corrupted DLL issues in releases
Write-Host "Oodle DLL will be downloaded on-demand by the app" -ForegroundColor Cyan

# Copy licenses and basic docs
$docs = @(
    "README.md",
    "CHANGELOG.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "LICENSE-GPL"
)
foreach ($doc in $docs) {
    $p = Join-Path $root $doc
    if (Test-Path $p) { Copy-Item -LiteralPath $p -Destination (Join-Path $distDir (Split-Path $p -Leaf)) -Force }
}

# Copy fonts/palettes if present for custom UI features
$maybeDirs = @(
    (Join-Path $root "repak-x\fonts"),
    (Join-Path $root "repak-x\palettes")
)
foreach ($d in $maybeDirs) {
    if (Test-Path $d) {
        $dest = Join-Path $distDir (Split-Path $d -Leaf)
        Copy-Item -Path (Join-Path $d "*") -Destination $dest -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Optionally zip the dist
if ($Zip) {
    $zipPath = Join-Path $distRoot ("$appFolderName.zip")
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::CreateFromDirectory($distDir, $zipPath)
    Write-Host "Created archive: $zipPath"
}

Write-Host "Done. Distribution ready at: $distDir"
