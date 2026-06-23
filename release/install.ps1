# install.ps1
$ErrorActionPreference = "Stop"

# 1. Target configurations
$repoOwner   = "Abdullah-Masood-05"
$repoName    = "PoIO-Consensus-Algorithm"
$tag         = "v0.3.0"
$exeName     = "poio.exe"
$installDir  = "$env:USERPROFILE\.poio"

# 2. Construct the direct asset download URL (Bypasses GitHub API rate limits)
$downloadUrl = "https://github.com/$repoOwner/$repoName/releases/download/$tag/$exeName"

# 3. Create destination folder if it doesn't exist
if (-not (Test-Path -Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir | Out-Null
}

# 4. Stream and write the executable binary locally
Write-Host "Downloading poio.exe v0.3.0 to $installDir..." -ForegroundColor Cyan
Invoke-WebRequest -Uri $downloadUrl -OutFile "$installDir\$exeName" -UseBasicParsing

# 5. Integrate target directory with the local User PATH
Write-Host "Registering environment variables..." -ForegroundColor Cyan
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")

if ($currentPath -split ';' -notcontains $installDir) {
    [Environment]::SetEnvironmentVariable("Path", $currentPath + ";$installDir", "User")
    Write-Host "Successfully added $installDir to your User PATH." -ForegroundColor Green
} else {
    Write-Host "$installDir is already registered in your PATH environment." -ForegroundColor Yellow
}

Write-Host "`nInstallation complete! Please RESTART your terminal window to call 'poio' globally." -ForegroundColor Green


# # install.ps1
# $ErrorActionPreference = "Stop"

# # 1. Target configurations
# $repoOwner = "BazilSuhail"
# $repoName = "PoIO-Consensus-Algorithm"
# $exeName = "poio.exe"
# $installDir = "$env:USERPROFILE\.poio"

# # 2. Fetch the download URL for poio.exe from the v0.2.0 GitHub Release
# Write-Host "Fetching v0.2.0 release assets from GitHub..." -ForegroundColor Cyan
# $url = "https://api.github.com/repos/$repoOwner/$repoName/releases/tags/v0.2.0"
# $response = Invoke-RestMethod -Uri $url -UseBasicParsing
# $asset = $response.assets | Where-Object { $_.name -eq $exeName }

# if (-not $asset) {
#     throw "Could not find '$exeName' in the v0.2.0 release assets."
# }

# $downloadUrl = $asset.browser_download_url

# # 3. Create destination folder if it doesn't exist
# if (-not (Test-Path -Path $installDir)) {
#     New-Item -ItemType Directory -Path $installDir | Out-Null
# }

# # 4. Stream and write the executable binary locally
# Write-Host "Downloading poio.exe to $installDir..." -ForegroundColor Cyan
# Invoke-WebRequest -Uri $downloadUrl -OutFile "$installDir\$exeName" -UseBasicParsing

# # 5. Integrate target directory with the local User PATH
# Write-Host "Registering environment variables..." -ForegroundColor Cyan
# $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")

# if ($currentPath -split ';' -notcontains $installDir) {
#     [Environment]::SetEnvironmentVariable("Path", $currentPath + ";$installDir", "User")
#     Write-Host "Successfully added $installDir to your User PATH." -ForegroundColor Green
# } else {
#     Write-Host "$installDir is already registered in your PATH environment." -ForegroundColor Yellow
# }

# Write-Host "`nInstallation complete! Please RESTART your terminal window to call 'poio' globally." -ForegroundColor Green