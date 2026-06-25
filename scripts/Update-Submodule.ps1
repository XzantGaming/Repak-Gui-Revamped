$rootPath = Resolve-Path "$PSScriptRoot/.."
$submoduleFolder = "UAssetToolRivals"
$submodulePath = Join-Path $rootPath $submoduleFolder

# Ensure we are in the root
Set-Location $rootPath

if (-not (Test-Path "$submodulePath")) {
    Write-Error "Submodule directory '$submodulePath' not found. Run Init-Submodule.ps1 first."
    exit
}

Write-Host "Fetching latest changes for '$submoduleFolder'..." -ForegroundColor Cyan

# Update the submodule to track its remote branch (aot, per .gitmodules)
git submodule update --remote --merge $submoduleFolder

Write-Host "Done." -ForegroundColor Green
Write-Host "NOTE: If the submodule version changed, you need to commit the change in this parent repository." -ForegroundColor Yellow
