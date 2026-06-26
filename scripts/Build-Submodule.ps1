$ErrorActionPreference = "Stop"

$rootPath = Resolve-Path "$PSScriptRoot/.."
Set-Location $rootPath

# Paths
$projectPath = Join-Path $rootPath "UAssetToolRivals\src\UAssetTool\UAssetToolNative.csproj"
$targetSidecarPath = Join-Path $rootPath "target\uassettool\UAssetTool.dll"
$targetDir = Split-Path $targetSidecarPath -Parent

# Validation
if (-not (Test-Path $projectPath)) {
    Write-Error "Could not find UAssetTool project at: $projectPath"
    Write-Host "Make sure the submodule is initialized: .\scripts\Init-Submodule.ps1" -ForegroundColor Yellow
    exit 1
}

Write-Host "--- BUILDING UASSETTOOL SIDECAR ---" -ForegroundColor Cyan
Write-Host "Project: $projectPath"
Write-Host "Output:  $targetSidecarPath"

# Check for dotnet
if (-not (Get-Command "dotnet" -ErrorAction SilentlyContinue)) {
    Write-Error "The 'dotnet' command is not found. Please install the .NET SDK."
    exit 1
}

# Build and Publish
# We use 'dotnet publish' to create a single-file, self-contained executable
Write-Host "`nRunning dotnet publish..." -ForegroundColor Cyan
try {
    dotnet publish $projectPath `
        -c Release `
        -r win-x64 `
        -o "$rootPath\temp_build"
}
catch {
    Write-Error "Build failed. Check the output above."
    exit 1
}

# Move and Rename
$builtDll = Join-Path "$rootPath\temp_build" "UAssetTool.dll"

if (Test-Path $builtDll) {
    if (-not (Test-Path $targetDir)) {
        New-Item -ItemType Directory -Path $targetDir | Out-Null
    }
    
    Write-Host "Copying binary to target/uassettool..." -ForegroundColor Cyan
    Copy-Item -Path $builtDll -Destination $targetSidecarPath -Force
    
    # Cleanup
    Remove-Item "$rootPath\temp_build" -Recurse -Force
    
    Write-Host "SUCCESS! Sidecar updated." -ForegroundColor Green
}
else {
    Write-Error "Build succeeded but could not find output library at $builtDll"
    exit 1
}
