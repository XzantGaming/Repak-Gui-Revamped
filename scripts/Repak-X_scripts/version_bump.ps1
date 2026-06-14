# Version Bump Utility
# Shared function for push scripts to handle [run-ci] releases with version bumping

function Invoke-VersionBump {
    param(
        [string]$RepoRoot
    )
    
    # Ask about [run-ci]
    Write-Host ""
    $runCI = Read-Host "Is this a [run-ci] release commit? (Y/n)"
    
    if ($runCI -ne "Y" -and $runCI -ne "y" -and $runCI -ne "") {
        return @{ RunCI = $false; Version = $null }
    }
    
    if ($runCI -eq "") {
        return @{ RunCI = $false; Version = $null }
    }
    
    # Read current version from Cargo.toml
    $cargoPath = Join-Path $RepoRoot "Cargo.toml"
    $cargoContent = Get-Content $cargoPath -Raw
    $currentVersion = [regex]::Match($cargoContent, 'version\s*=\s*"(\d+\.\d+\.\d+)"').Groups[1].Value
    
    if (-not $currentVersion) {
        Write-Host "ERROR: Could not read current version from Cargo.toml" -ForegroundColor Red
        return @{ RunCI = $false; Version = $null }
    }
    
    Write-Host ""
    Write-Host "Current version: $currentVersion" -ForegroundColor Yellow
    
    # Parse current version for suggestions
    $parts = $currentVersion -split '\.'
    $major = [int]$parts[0]
    $minor = [int]$parts[1]
    $patch = [int]$parts[2]
    
    $patchBump = "$major.$minor.$($patch + 1)"
    $minorBump = "$major.$($minor + 1).0"
    $majorBump = "$($major + 1).0.0"
    
    Write-Host ""
    Write-Host "Version bump options:" -ForegroundColor Cyan
    Write-Host "  [1] Patch: $patchBump (bug fixes)" -ForegroundColor Gray
    Write-Host "  [2] Minor: $minorBump (new features)" -ForegroundColor Gray
    Write-Host "  [3] Major: $majorBump (breaking changes)" -ForegroundColor Gray
    Write-Host "  [4] Custom version" -ForegroundColor Gray
    Write-Host ""
    
    $choice = Read-Host "Select option (1-4) or enter version directly"
    
    $newVersion = switch ($choice) {
        "1" { $patchBump }
        "2" { $minorBump }
        "3" { $majorBump }
        "4" { 
            $custom = Read-Host "Enter custom version (e.g. 1.2.3)"
            $custom
        }
        default {
            # Check if they entered a version directly
            if ($choice -match '^\d+\.\d+\.\d+$') {
                $choice
            } else {
                Write-Host "Invalid choice. Using patch bump." -ForegroundColor Yellow
                $patchBump
            }
        }
    }
    
    # Validate semver format
    if ($newVersion -notmatch '^\d+\.\d+\.\d+$') {
        Write-Host "Invalid version format! Use x.y.z" -ForegroundColor Red
        return @{ RunCI = $false; Version = $null }
    }
    
    # Check if CHANGELOG.md has an entry for this version
    $changelogPath = Join-Path $RepoRoot "CHANGELOG.md"
    if (Test-Path $changelogPath) {
        $changelogContent = Get-Content $changelogPath -Raw
        if ($changelogContent -notmatch "\[$newVersion\]") {
            Write-Host ""
            Write-Host "WARNING: No CHANGELOG.md entry found for version $newVersion" -ForegroundColor Yellow
            $continueAnyway = Read-Host "Continue anyway? (y/N)"
            if ($continueAnyway -ne "y" -and $continueAnyway -ne "Y") {
                Write-Host "Aborted. Please update CHANGELOG.md first." -ForegroundColor Red
                return @{ RunCI = $false; Version = $null }
            }
        } else {
            Write-Host "CHANGELOG.md entry found for v$newVersion" -ForegroundColor Green
        }
    }
    
    Write-Host ""
    Write-Host "Updating version $currentVersion -> $newVersion ..." -ForegroundColor Cyan
    
    # 1. Update Cargo.toml (workspace version only, not dependency versions)
    $cargoLines = Get-Content $cargoPath
    $inWorkspacePackage = $false
    $updatedLines = @()
    foreach ($line in $cargoLines) {
        if ($line -match '^\[workspace\.package\]') {
            $inWorkspacePackage = $true
        } elseif ($line -match '^\[') {
            $inWorkspacePackage = $false
        }
        if ($inWorkspacePackage -and $line -match '^version\s*=\s*"[\d.]+"') {
            $line = $line -replace '(version\s*=\s*")[\d.]+(")', "`${1}$newVersion`${2}"
        }
        $updatedLines += $line
    }
    $updatedLines -join "`n" | Set-Content $cargoPath -NoNewline
    Write-Host "  [OK] Cargo.toml" -ForegroundColor Green
    
    # 2. Update tauri.conf.json
    $tauriConfPath = Join-Path $RepoRoot "repak-x\tauri.conf.json"
    if (Test-Path $tauriConfPath) {
        $tauriContent = Get-Content $tauriConfPath -Raw
        $tauriContent = $tauriContent -replace '("version"\s*:\s*")[\d.]+(")', "`${1}$newVersion`${2}"
        Set-Content $tauriConfPath $tauriContent -NoNewline
        Write-Host "  [OK] tauri.conf.json" -ForegroundColor Green
    }
    
    # 3. Update package.json
    $packageJsonPath = Join-Path $RepoRoot "repak-x\package.json"
    if (Test-Path $packageJsonPath) {
        $packageContent = Get-Content $packageJsonPath -Raw
        $packageContent = $packageContent -replace '("version"\s*:\s*")[\d.]+(")', "`${1}$newVersion`${2}"
        Set-Content $packageJsonPath $packageContent -NoNewline
        Write-Host "  [OK] package.json" -ForegroundColor Green
    }
    
    # 4. Run cargo check to update Cargo.lock
    Write-Host ""
    Write-Host "Updating Cargo.lock..." -ForegroundColor Cyan
    Push-Location $RepoRoot
    # cargo writes compiler warnings to stderr; under the caller's $ErrorActionPreference='Stop'
    # the first warning line surfaces as a terminating NativeCommandError. Neutralize EAP for the
    # duration and discard all output so a noisy (but harmless) warning can't abort the release.
    $prevEAP = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    cargo check --quiet 2>&1 | Out-Null
    $ErrorActionPreference = $prevEAP
    Pop-Location
    Write-Host "  [OK] Cargo.lock" -ForegroundColor Green
    
    # 5. Stage version files
    Write-Host ""
    Write-Host "Staging version files..." -ForegroundColor Cyan
    Push-Location $RepoRoot
    git add "Cargo.toml"
    git add "Cargo.lock"
    git add "repak-x/tauri.conf.json"
    git add "repak-x/package.json"
    Pop-Location
    
    Write-Host ""
    Write-Host "Version bump complete! Commit message will be prefixed with [run-ci]" -ForegroundColor Green
    
    return @{ RunCI = $true; Version = $newVersion }
}

