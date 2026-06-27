param(
    [switch]$NonInteractive
)

$rootPath = Resolve-Path "$PSScriptRoot/.."
$submodulePath = Join-Path $rootPath "UAssetToolRivals"
$repoUrl = "https://github.com/XzantGaming/UAssetToolRivals.git"

# Ensure we are in the root for git commands to work as expected with the correct relative paths
Set-Location $rootPath

# Check if the folder exists
if (Test-Path $submodulePath) {
    Write-Host "Directory '$submodulePath' already exists." -ForegroundColor Yellow
    
    # Check if it's already a submodule/git repo
    if (Test-Path "$submodulePath\.git") {
        Write-Host "It appears to be initialized already. Running update to ensure integrity..." -ForegroundColor Cyan
    }
    else {
        $isEmpty = (Get-ChildItem $submodulePath -Force | Measure-Object).Count -eq 0
        if ($isEmpty) {
            Write-Host "Directory '$submodulePath' is empty. Removing it so git can cleanly pull the submodule." -ForegroundColor Cyan
            Remove-Item $submodulePath -Force
        }
        else {
            Write-Host "WARNING: '$submodulePath' exists but is NOT a git repository." -ForegroundColor Red
            Write-Host "We need to remove this folder to add it as a submodule." -ForegroundColor Yellow
            
            if ($NonInteractive) {
                Write-Host "Non-interactive mode: Automatically removing existing folder to fix submodule." -ForegroundColor Yellow
                $response = 'y'
            }
            else {
                $response = Read-Host "Do you want to backup and delete the existing folder? (y/n)"
            }
            
            if ($response -eq 'y') {
                $backupName = "${submodulePath}_backup_$(Get-Date -Format 'yyyyMMdd_HHmmss')"
                Rename-Item $submodulePath $backupName
                Write-Host "Backed up to $backupName" -ForegroundColor Green
            }
            else {
                Write-Host "Operation aborted by user." -ForegroundColor Red
                exit
            }
        }
    }
}

Write-Host "Adding submodule from $repoUrl..." -ForegroundColor Cyan
# 'git submodule add' might fail if it's already in index but not on disk, 
# so we try 'update --init' as fallback or primary if add isn't needed.

# If .gitmodules exists and has the module, we just need update --init
$moduleExists = $false
if (Test-Path ".gitmodules") {
    if (Select-String -Path ".gitmodules" -Pattern "UAssetToolRivals" -Quiet) {
        $moduleExists = $true
    }
}

if ($moduleExists) {
    Write-Host "Submodule already defined in .gitmodules. Ensuring it tracks 'aot' branch." -ForegroundColor Cyan
    git submodule set-branch -b aot UAssetToolRivals
}
else {
    git submodule add -b aot $repoUrl UAssetToolRivals
}

Write-Host "Initializing and pulling latest commit from branch..." -ForegroundColor Cyan
git submodule update --init --recursive

Write-Host "Submodule setup complete!" -ForegroundColor Green
