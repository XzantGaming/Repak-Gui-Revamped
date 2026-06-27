# ============================================
# Repak GUI - Contributor Build Script
# ============================================
# This script builds the entire project from scratch:
# - C# projects (UAssetBridge, StaticMeshSerializeSizeFixer)
# - Rust workspace (frontend + backend)
# - All dependencies and tools
#
# Usage: .\build_contributor.ps1 [-Configuration <debug|release>]
# ============================================

param(
    [ValidateSet("debug", "release")]
    [string]$Configuration = "release"
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

function Kill-Process-Safe {
    param([string]$Name)
    $procs = Get-Process -Name $Name -ErrorAction SilentlyContinue
    if ($procs) {
        Write-Info "Stopping $Name process(es) to release file locks..."
        $procs | Stop-Process -Force -ErrorAction SilentlyContinue
    }
}

# Get workspace root (scripts are in scripts/Repak-X_scripts/, so go up 2 levels)
$scriptDir = Split-Path -Parent $PSCommandPath
$workspaceRoot = Split-Path -Parent (Split-Path -Parent $scriptDir)
Push-Location $workspaceRoot

try {
    Write-Step "Repak GUI - Full Contributor Build"
    Write-Info "Configuration: $Configuration"
    Write-Info "Workspace: $workspaceRoot"
    Write-Host ""
    
    # ============================================
    # Step 0: Ensure Submodules
    # ============================================
    Write-Step "[0/4] Checking Submodules"
    Write-Info "Ensuring UAssetToolRivals submodule is initialized..."
    
    $initScript = Join-Path $workspaceRoot "scripts\Init-Submodule.ps1"
    if (Test-Path $initScript) {
        & $initScript -NonInteractive
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "Submodule init script returned error code $LASTEXITCODE"
        }
        else {
            Write-Success "Submodule initialized"
        }
    }
    else {
        Write-Warning "Init-Submodule.ps1 not found at $initScript"
    }

    # ============================================
    # Step 1: Check Prerequisites
    # ============================================
    Write-Step "[1/4] Checking Prerequisites"
    
    # Check Rust
    Write-Info "Checking Rust installation..."
    $rustVersion = & cargo --version 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Custom "Rust/Cargo not found! Install from https://rustup.rs/"
        exit 1
    }
    Write-Success "Rust: $rustVersion"

    # Check .NET SDK
    Write-Info "Checking .NET SDK installation..."
    $dotnetVersion = & dotnet --version 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Custom ".NET SDK not found! Install .NET 8.0 SDK from https://dotnet.microsoft.com/download"
        exit 1
    }
    Write-Success ".NET SDK: $dotnetVersion"

    # Check Node.js (for frontend)
    Write-Info "Checking Node.js installation..."
    $nodeVersion = & node --version 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Custom "Node.js not found! Install from https://nodejs.org/"
        exit 1
    }
    Write-Success "Node.js: $nodeVersion"

    # ============================================
    # Step 2: Build C# Projects
    # ============================================
    Write-Step "[2/4] Building C# Projects"

    # Build UAssetTool from UassetToolRivals submodule
    Write-Info "Building UAssetTool.exe from submodule..."
    
    # Ensure no previous instances are running and locking files
    Kill-Process-Safe "UAssetTool"
    
    $toolProject = Join-Path $workspaceRoot "UAssetToolRivals\src\UAssetTool\UAssetToolNative.csproj"
    if (Test-Path $toolProject) {
        $toolOutput = Join-Path $workspaceRoot "target\uassettool"
        # Clean stale artifacts (e.g. ue4-dds-tools, .pdb) from previous builds
        if (Test-Path $toolOutput) {
            Remove-Item -Path $toolOutput -Recurse -Force -ErrorAction SilentlyContinue
            Write-Info "Cleaned stale target\uassettool directory"
        }
        New-Item -ItemType Directory -Force -Path $toolOutput | Out-Null
        
        # Build UAssetToolNative AOT library from submodule
        Write-Info "Building UAssetToolNative (AOT DLL) from submodule..."
        & dotnet publish $toolProject `
            -c Release `
            -r win-x64 `
            -p:DebugType=none `
            -p:DebugSymbols=false `
            -o $toolOutput
        if ($LASTEXITCODE -ne 0) {
            Write-Error-Custom "UAssetTool AOT library build failed!"
            exit 1
        }
        
        $toolDll = Join-Path $toolOutput "UAssetTool.dll"
        if (Test-Path $toolDll) {
            Write-Success "UAssetTool.dll built successfully"
            Write-Info "Location: $toolDll"
        }
        else {
            Write-Error-Custom "UAssetTool.dll not found after build!"
            exit 1
        }
        
        # NOTE: hash_helper.exe is no longer needed - CityHash64 is now implemented natively in C#
        # NOTE: UE4-DDS-Tools (Python) is no longer needed
        # Texture conversion now uses native C# UAssetTool (TEXTURE_IMPLEMENTATION = "csharp")
    }
    else {
        Write-Warning "UAssetTool project not found at $toolProject"
    }

    # ============================================
    # Step 3: Install Frontend Dependencies
    # ============================================
    Write-Step "[3/4] Installing Frontend Dependencies"
    
    $frontendDir = Join-Path $workspaceRoot "repak-x"
    if (Test-Path $frontendDir) {
        Push-Location $frontendDir
        
        Write-Info "Running npm install..."
        & npm install
        if ($LASTEXITCODE -ne 0) {
            Pop-Location
            Write-Error-Custom "npm install failed!"
            exit 1
        }
        Write-Success "Frontend dependencies installed"
        
        Pop-Location
    }
    else {
        Write-Error-Custom "Frontend directory not found at $frontendDir"
        exit 1
    }

    # ============================================
    # Step 4: Build Rust Workspace (Backend + Tauri)
    # ============================================
    # NOTE: Tauri's beforeBuildCommand in tauri.conf.json automatically
    # runs "npm-build.bat" which builds the React frontend via Vite.
    # We don't need a separate frontend build step - Tauri handles it!
    Write-Step "[4/4] Building Rust Workspace (Backend + Tauri)"

    Write-Info "Building Tauri application (includes frontend build via beforeBuildCommand)..."
    Push-Location $frontendDir

    $tauriArgs = @("build", "--no-bundle")
    if ($Configuration -eq "debug") {
        $tauriArgs += "--debug"
    }
    
    # Pre-seed UAssetTool to the target specific directory so build.rs finds it and can skip rebuilding
    $profileDir = if ($Configuration -eq "release") { "release" } else { "debug" }
    $parentProfileDir = Join-Path $workspaceRoot "target\$profileDir"
    
    if (-not (Test-Path $parentProfileDir)) {
        New-Item -ItemType Directory -Force -Path $parentProfileDir | Out-Null
    }
    
    $srcTool = Join-Path $workspaceRoot "target\uassettool\UAssetTool.dll"
    if (Test-Path $srcTool) {
        Copy-Item -Path (Join-Path $workspaceRoot "target\uassettool\*") -Destination $parentProfileDir -Recurse -Force
        Write-Info "Pre-seeded UAssetTool binaries for rust build"
    }
    
    # Tell build.rs to skip building UAssetTool
    $env:SKIP_UASSET_TOOL_BUILD = "1"

    & npx tauri @tauriArgs
    $tauriExitCode = $LASTEXITCODE

    # Clean up env var
    Remove-Item Env:\SKIP_UASSET_TOOL_BUILD

    Pop-Location

    if ($tauriExitCode -ne 0) {
        Write-Error-Custom "Tauri build failed!"
        exit 1
    }
    Write-Success "Tauri application built successfully"

    # ============================================
    # Build Complete - Summary
    # ============================================
    Write-Step "Build Complete!"
    
    $profileDir = if ($Configuration -eq "release") { "release" } else { "debug" }
    $exePath = Join-Path $workspaceRoot "target\$profileDir\REPAK-X.exe"
    $toolPath = Join-Path $workspaceRoot "target\uassettool\UAssetTool.dll"
    
    Write-Host ""
    Write-Host "Built Artifacts:" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    
    if (Test-Path $exePath) {
        Write-Success "Main Application: $exePath"
    }
    else {
        Write-Warning "Main Application not found at: $exePath"
    }
    
    if (Test-Path $toolPath) {
        Write-Success "UAssetTool: $toolPath"
    }
    else {
        Write-Warning "UAssetTool not found at: $toolPath"
    }
    
    Write-Host ""
    Write-Host "To run the application:" -ForegroundColor Yellow
    Write-Host "  .\target\$profileDir\REPAK-X.exe" -ForegroundColor White
    Write-Host ""
    Write-Host "To create a distribution package:" -ForegroundColor Yellow
    Write-Host "  .\package_release.ps1 -Configuration $Configuration" -ForegroundColor White
    Write-Host ""

}
catch {
    Write-Error-Custom "Build failed with error: $_"
    exit 1
}
finally {
    Pop-Location
}
