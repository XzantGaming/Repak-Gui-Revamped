$rootPath = Resolve-Path "$PSScriptRoot/.."
Set-Location $rootPath

$submodulePath = "UAssetToolRivals"

if (-not (Test-Path "$submodulePath")) {
    Write-Error "Submodule directory '$submodulePath' not found."
    exit
}

Write-Host "--- PROCESSING SUBMODULE: $submodulePath ---" -ForegroundColor Cyan
Push-Location $submodulePath

# Check for local changes in submodule
$status = git status --porcelain
if ($status) {
    Write-Host "Uncommitted changes found in submodule:" -ForegroundColor Yellow
    git status -s
    
    $doCommit = Read-Host "Do you want to commit and push these changes to the remote submodule repo? (y/n)"
    if ($doCommit -eq 'y') {
        $msg = Read-Host "Enter commit message"
        if (-not [string]::IsNullOrWhiteSpace($msg)) {
            git add .
            git commit -m "$msg"
            # Explicitly pushing head to origin since detached head is common in submodules
            # The submodule now lives on the 'aot' branch (NativeAOT FFI build).
            git push origin HEAD:aot
            Write-Host "Changes pushed to submodule remote." -ForegroundColor Green
        }
    }
}
else {
    Write-Host "No local changes in submodule." -ForegroundColor Gray
}

Pop-Location

Write-Host "`n--- PROCESSING PARENT REPO ---" -ForegroundColor Cyan
# Check if parent needs to update its pointer
$parentDiff = git diff --name-only $submodulePath
$parentStaged = git diff --cached --name-only $submodulePath

if ($parentDiff -or $parentStaged) {
    Write-Host "The parent repository sees a new version (commit hash) for '$submodulePath'." -ForegroundColor Yellow
    $doUpdate = Read-Host "Do you want to update the parent repository to point to this new submodule version? (y/n)"
    if ($doUpdate -eq 'y') {
        git add $submodulePath
        $commitMsg = "chore: update submodule $submodulePath"
        git commit -m $commitMsg
        Write-Host "Parent repository updated. Remember to 'git push' the parent repo." -ForegroundColor Green
    }
}
else {
    Write-Host "Parent repository is already in sync with the submodule." -ForegroundColor Green
}
