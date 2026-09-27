# Proxync One-Line Installer for Windows
# Usage:
#   irm https://raw.githubusercontent.com/Inilax/Proxync/main/scripts/install.ps1 | iex
#   & { irm https://raw.githubusercontent.com/Inilax/Proxync/main/scripts/install.ps1 } -Gui
param(
    [switch]$Gui
)

$ErrorActionPreference = "Stop"
$Repo = "Inilax/Proxync"

Write-Host "==> Installing Proxync for Windows (x64)..." -ForegroundColor Cyan

$InstallDir = "$env:LOCALAPPDATA\Programs\Proxync\bin"
if (!(Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$Version = "v0.2.4"
try {
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
    if ($Release.tag_name) {
        $Version = $Release.tag_name
    }
} catch {
    # Fallback to default version if API is rate limited
}

$ExeUrl = "https://github.com/$Repo/releases/download/$Version/proxync-windows-x86_64.exe"
$TargetPath = Join-Path $InstallDir "proxync.exe"

Write-Host "==> Fetching Proxync CLI ($Version)..."
try {
    Invoke-WebRequest -Uri $ExeUrl -OutFile $TargetPath -UseBasicParsing
    Write-Host "✓ Downloaded release binary" -ForegroundColor Green
} catch {
    # Check if local release or debug binary exists in repo
    $LocalBin = "packages\cli\target\release\proxync.exe"
    $LocalDebug = "packages\cli\target\debug\proxync.exe"
    if (Test-Path $LocalBin) {
        Copy-Item $LocalBin $TargetPath -Force
        Write-Host "✓ Installed from local release build" -ForegroundColor Green
    } elseif (Test-Path $LocalDebug) {
        Copy-Item $LocalDebug $TargetPath -Force
        Write-Host "✓ Installed from local debug build" -ForegroundColor Green
    } else {
        Write-Host "Warning: Could not download $ExeUrl and no local binary found." -ForegroundColor Yellow
        Write-Host "Building locally via cargo..."
        cargo build --release --manifest-path="packages\cli\Cargo.toml"
        Copy-Item $LocalBin $TargetPath -Force
    }
}

# Add to User PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path += ";$InstallDir"
    Write-Host "✓ Added $InstallDir to User PATH" -ForegroundColor Green
}

Write-Host "✓ Proxync CLI installed to $TargetPath" -ForegroundColor Green

# Install Desktop GUI if requested
if ($Gui) {
    Write-Host "==> Fetching Proxync Desktop GUI Installer..." -ForegroundColor Cyan
    $VerNum = $Version.TrimStart('v')
    $GuiInstallerUrl = "https://github.com/$Repo/releases/download/$Version/Proxync_${VerNum}_x64-setup.exe"
    $TempInstaller = Join-Path $env:TEMP "proxync-gui-setup.exe"
    try {
        Invoke-WebRequest -Uri $GuiInstallerUrl -OutFile $TempInstaller -UseBasicParsing
        Write-Host "==> Launching Desktop Installer..." -ForegroundColor Cyan
        Start-Process -FilePath $TempInstaller -Wait
        Write-Host "✓ Proxync Desktop GUI installed" -ForegroundColor Green
    } catch {
        Write-Host "Desktop installer not yet available from GitHub releases." -ForegroundColor Yellow
        Write-Host "You can launch GUI via dev mode: npm run dev"
    }
}

Write-Host "`nAll set! Restart your terminal or run:" -ForegroundColor Green
Write-Host "  proxync --help" -ForegroundColor Yellow
Write-Host "  proxync scan" -ForegroundColor Yellow
