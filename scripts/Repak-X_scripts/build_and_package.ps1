# ============================================
# Repak GUI - Build and Package for Distribution
# ============================================
# This script builds the entire project and creates a distribution package
# ready to share with users.
#
# Usage: .\build_and_package.ps1 [-Configuration <debug|release>] [-Zip]
# ============================================

param(
    [ValidateSet("debug", "release")]
    [string]$Configuration = "release",
    [switch]$Zip
)

$ErrorActionPreference = "Stop"

# Color output functions
function Write-Step {
    param([string]$Message)
    Write-Host "`n========================================" -ForegroundColor Cyan
    Write-Host $Message -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
}

function Write-Success {
    param([string]$Message)
    Write-Host "[OK] $Message" -ForegroundColor Green
}

function Write-Error-Custom {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

function Write-Info {
    param([string]$Message)
    Write-Host "-> $Message" -ForegroundColor Yellow
}

function Get-Version {
    param([string]$CargoTomlPath)
    $content = Get-Content -Path $CargoTomlPath -Raw
    $m = [regex]::Match($content, '(?m)^version\s*=\s*"([^"]+)"')
    if ($m.Success) { return $m.Groups[1].Value }
    return "0.0.0"
}

# Get workspace root (scripts are in scripts/Repak-X_scripts/, so go up 2 levels)
$scriptDir = Split-Path -Parent $PSCommandPath
$workspaceRoot = Split-Path -Parent (Split-Path -Parent $scriptDir)
Push-Location $workspaceRoot

try {
    Write-Step "Repak GUI - Build and Package for Distribution"
    Write-Info "Configuration: $Configuration"
    Write-Info "Workspace: $workspaceRoot"
    Write-Host ""

    # ============================================
    # Step 1: Run Full Build
    # ============================================
    Write-Step "[1/2] Building Project"
    Write-Info "Running full contributor build..."
    
    $buildScript = Join-Path $scriptDir "build_contributor.ps1"
    & $buildScript -Configuration $Configuration
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Custom "Build failed! Cannot create distribution package."
        exit 1
    }
    
    Write-Success "Build completed successfully"

    # ============================================
    # Step 2: Create Distribution Package
    # ============================================
    Write-Step "[2/2] Creating Distribution Package"
    
    # Determine version
    $cargoRoot = Join-Path $workspaceRoot "Cargo.toml"
    $version = Get-Version -CargoTomlPath $cargoRoot
    Write-Info "Version: $version"
    
    # Setup paths
    $profileDir = if ($Configuration -eq "release") { "release" } else { "debug" }
    $targetDir = Join-Path $workspaceRoot "target\$profileDir"
    $distRoot = Join-Path $workspaceRoot "dist"
    $appFolderName = "Repak-X-v$version"
    $distDir = Join-Path $distRoot $appFolderName
    
    Write-Info "Creating distribution folder: $distDir"
    
    # Clean and create dist directory
    if (Test-Path $distDir) {
        Remove-Item -Path $distDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null
    
    # ============================================
    # Copy Main Application
    # ============================================
    Write-Info "Copying main application..."
    $exePath = Join-Path $targetDir "REPAK-X.exe"
    if (Test-Path $exePath) {
        Copy-Item -LiteralPath $exePath -Destination (Join-Path $distDir "REPAK-X.exe") -Force
        Write-Success "Copied REPAK-X.exe"
    }
    else {
        Write-Error-Custom "REPAK-X.exe not found at $exePath"
        exit 1
    }
    
    # ============================================
    # Copy UAssetTool (unified asset tool)
    # ============================================
    Write-Info "Copying UAssetTool..."
    $toolDir = Join-Path $workspaceRoot "target\uassettool"
    if (Test-Path $toolDir) {
        $destToolDir = Join-Path $distDir "uassettool"
        New-Item -ItemType Directory -Force -Path $destToolDir | Out-Null
        Copy-Item -Path (Join-Path $toolDir "*") -Destination $destToolDir -Recurse -Force
        # Clean out debug symbols and legacy tools that shouldn't ship
        Get-ChildItem -Path $destToolDir -Filter "*.pdb" -Recurse | Remove-Item -Force -ErrorAction SilentlyContinue
        $ddsTools = Join-Path $destToolDir "ue4-dds-tools"
        if (Test-Path $ddsTools) { Remove-Item -Path $ddsTools -Recurse -Force -ErrorAction SilentlyContinue }
        Write-Success "Copied UAssetTool (cleaned .pdb + ue4-dds-tools)"
    }
    else {
        Write-Warning "UAssetTool not found at $toolDir - asset pipeline will be disabled"
    }
    
    # NOTE: UAssetMeshFixer has been merged into UAssetTool (in uassettool folder)
    # No separate copy needed - UAssetTool handles all asset operations
    
    # NOTE: UE4-DDS-Tools (Python) is no longer needed
    # Texture conversion now uses native C# UAssetTool (TEXTURE_IMPLEMENTATION = "csharp")
    
    # ============================================
    # Oodle DLL - Downloaded on demand
    # ============================================
    # NOTE: oo2core_9_win64.dll is now downloaded automatically by the app on first use
    # This avoids issues with corrupted DLLs being bundled in releases
    Write-Info "Oodle DLL will be downloaded on-demand by the app (not bundled)"
    
    # NOTE: Data folder (character_data.json) is NOT needed in distribution
    # The app generates this file in the user's AppData/Roaming directory at runtime
    
    # ============================================
    # Copy Documentation
    # ============================================
    Write-Info "Copying documentation..."
    $docs = @(
        "README.md",
        "CHANGELOG.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "LICENSE-GPL"
    )
    foreach ($doc in $docs) {
        $docPath = Join-Path $workspaceRoot $doc
        if (Test-Path $docPath) {
            Copy-Item -LiteralPath $docPath -Destination (Join-Path $distDir (Split-Path $doc -Leaf)) -Force
        }
    }
    Write-Success "Copied documentation"
    
    # ============================================
    # Copy Optional Assets
    # ============================================
    Write-Info "Copying optional assets..."
    $optionalDirs = @(
        (Join-Path $workspaceRoot "repak-x\fonts"),
        (Join-Path $workspaceRoot "repak-x\palettes")
    )
    foreach ($dir in $optionalDirs) {
        if (Test-Path $dir) {
            $destDir = Join-Path $distDir (Split-Path $dir -Leaf)
            New-Item -ItemType Directory -Force -Path $destDir | Out-Null
            Copy-Item -Path (Join-Path $dir "*") -Destination $destDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    
    # ============================================
    # Create README for Distribution
    # ============================================
    Write-Info "Creating distribution README..."
    $distReadme = @"
# Repak GUI v$version

## Installation

1. Extract all files to a folder of your choice
2. Run ``REPAK-X.exe``

## Requirements

- Windows x64
- Marvel Rivals installed
- .NET Runtime 8.0 or later (for UAssetBridge)

## What's Included

- ``REPAK-X.exe`` - Main application
- ``uassetbridge/`` - Texture processing tools (optional)
- ``tools/`` - Additional utilities
- Oodle compression library (downloaded automatically on first use)

## Usage

1. Launch ``REPAK-X.exe``
2. Drag and drop PAK mod files into the application
3. Configure settings as needed
4. Click "Install" to process and install mods

## Troubleshooting

- If texture processing fails, ensure .NET Runtime 8.0 is installed
- Check ``latest.log`` in the application directory for detailed error messages
- For support, visit: https://github.com/XzantGaming/Repak-X

## License

This software is licensed under GPL-3.0.
See LICENSE for details.
"@
    
    $distReadmePath = Join-Path $distDir "README_DIST.txt"
    Set-Content -Path $distReadmePath -Value $distReadme -Encoding UTF8
    Write-Success "Created distribution README"
    
    # ============================================
    # Calculate Package Size
    # ============================================
    $totalSize = (Get-ChildItem -Path $distDir -Recurse | Measure-Object -Property Length -Sum).Sum
    $sizeMB = [math]::Round($totalSize / 1MB, 2)
    
    # ============================================
    # Create ZIP Archive (Optional)
    # ============================================
    if ($Zip) {
        Write-Info "Creating ZIP archive..."
        $zipPath = Join-Path $distRoot "$appFolderName.zip"
        
        if (Test-Path $zipPath) {
            Remove-Item $zipPath -Force
        }
        
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [System.IO.Compression.ZipFile]::CreateFromDirectory($distDir, $zipPath)
        
        $zipSize = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
        Write-Success "Created ZIP archive: $zipPath ($zipSize MB)"
    }
    
    # ============================================
    # Build Complete - Summary
    # ============================================
    Write-Step "Distribution Package Complete!"
    
    Write-Host ""
    Write-Host "Package Details:" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "Version:        $version" -ForegroundColor White
    Write-Host "Configuration:  $Configuration" -ForegroundColor White
    Write-Host "Package Size:   $sizeMB MB" -ForegroundColor White
    Write-Host "Location:       $distDir" -ForegroundColor White
    
    if ($Zip) {
        Write-Host "ZIP Archive:    $zipPath" -ForegroundColor White
    }
    
    Write-Host ""
    Write-Host "Package Contents:" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    
    $files = Get-ChildItem -Path $distDir -Recurse -File
    $fileCount = $files.Count
    Write-Host "Total Files:    $fileCount" -ForegroundColor White
    
    Write-Host ""
    Write-Host "Main Components:" -ForegroundColor Yellow
    
    $components = @(
        @{Name = "Main Application"; Path = "REPAK-X.exe" },
        @{Name = "UAssetTool"; Path = "uassettool\UAssetTool.dll" }
        # Note: hash_helper.exe is no longer needed - CityHash64 is now implemented natively in UAssetTool
        # Note: Oodle DLL is downloaded on-demand by the app, not bundled
    )
    
    foreach ($component in $components) {
        $componentPath = Join-Path $distDir $component.Path
        if (Test-Path $componentPath) {
            $size = [math]::Round((Get-Item $componentPath).Length / 1MB, 2)
            Write-Host "  [OK] $($component.Name) ($size MB)" -ForegroundColor Green
        }
        else {
            Write-Host "  [MISSING] $($component.Name)" -ForegroundColor Yellow
        }
    }
    
    Write-Host ""
    Write-Host "Ready to distribute!" -ForegroundColor Green
    Write-Host ""
    Write-Host "To share this package:" -ForegroundColor Yellow
    if ($Zip) {
        Write-Host "  1. Upload the ZIP file: $appFolderName.zip" -ForegroundColor White
    }
    else {
        Write-Host "  1. Create a ZIP: .\build_and_package.ps1 -Zip" -ForegroundColor White
        Write-Host "  2. Or share the folder: $distDir" -ForegroundColor White
    }
    Write-Host ""

}
catch {
    Write-Error-Custom "Packaging failed with error: $_"
    exit 1
}
finally {
    Pop-Location
}
