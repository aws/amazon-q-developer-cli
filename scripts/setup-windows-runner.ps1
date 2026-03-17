#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Sets up a Windows Server 2019 self-hosted GitHub Actions runner with all
    dependencies needed for the kiro-cli Windows build workflow.
.DESCRIPTION
    Installs: 7-Zip, Git, Rust (stable), .NET SDK 8, WiX Toolset, Inno Setup 6,
    Visual Studio Build Tools (MSVC).
    Uses direct downloads — no winget dependency.
#>

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Write-Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }

# --- 7-Zip ---
Write-Step "Installing 7-Zip"
if (Get-Command 7z -ErrorAction SilentlyContinue) {
    Write-Host "7-Zip already installed, skipping."
} else {
    $installer = "$env:TEMP\7z-install.exe"
    Invoke-WebRequest -Uri "https://www.7-zip.org/a/7z2408-x64.exe" -OutFile $installer
    Start-Process -FilePath $installer -ArgumentList "/S" -Wait
    $env:PATH = "C:\Program Files\7-Zip;$env:PATH"
    [Environment]::SetEnvironmentVariable("PATH", "C:\Program Files\7-Zip;$([Environment]::GetEnvironmentVariable('PATH','Machine'))", "Machine")
    Write-Host "7-Zip installed."
}

# --- Git ---
Write-Step "Installing Git"
if (Get-Command git -ErrorAction SilentlyContinue) {
    Write-Host "Git already installed, skipping."
} else {
    $installer = "$env:TEMP\git-install.exe"
    Invoke-WebRequest -Uri "https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.2/Git-2.47.1.2-64-bit.exe" -OutFile $installer
    Start-Process -FilePath $installer -ArgumentList "/VERYSILENT","/NORESTART","/NOCANCEL","/SP-" -Wait
    $env:PATH = "C:\Program Files\Git\cmd;$env:PATH"
    [Environment]::SetEnvironmentVariable("PATH", "C:\Program Files\Git\cmd;$([Environment]::GetEnvironmentVariable('PATH','Machine'))", "Machine")
    Write-Host "Git installed."
}

# --- Visual Studio Build Tools (MSVC) ---
Write-Step "Installing Visual Studio Build Tools (MSVC)"
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$needsVS = $true
if (Test-Path $vsWhere) {
    $vsInstalls = & $vsWhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if ($vsInstalls) {
        Write-Host "MSVC Build Tools already installed, skipping."
        $needsVS = $false
    }
}
if ($needsVS) {
    $installer = "$env:TEMP\vs_buildtools.exe"
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vs_buildtools.exe" -OutFile $installer
    Start-Process -FilePath $installer -ArgumentList `
        "--add", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", `
        "--add", "Microsoft.VisualStudio.Component.VC.Tools.ARM64", `
        "--add", "Microsoft.VisualStudio.Component.Windows10SDK.19041", `
        "--quiet", "--norestart" -Wait
    Write-Host "MSVC Build Tools installed."
}

# --- Rust via rustup ---
Write-Step "Installing Rust (stable)"
if (Get-Command rustup -ErrorAction SilentlyContinue) {
    Write-Host "Rust already installed. Updating..."
    rustup update stable
} else {
    $installer = "$env:TEMP\rustup-init.exe"
    Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $installer
    Start-Process -FilePath $installer -ArgumentList "-y","--default-toolchain","stable" -Wait
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    [Environment]::SetEnvironmentVariable("PATH", "$env:USERPROFILE\.cargo\bin;$([Environment]::GetEnvironmentVariable('PATH','User'))", "User")
}

Write-Step "Adding Rust targets"
rustup target add x86_64-pc-windows-msvc
rustup target add aarch64-pc-windows-msvc

# --- .NET SDK 8 ---
Write-Step "Installing .NET SDK 8"
if (Get-Command dotnet -ErrorAction SilentlyContinue) {
    Write-Host ".NET SDK already installed, skipping."
} else {
    $installer = "$env:TEMP\dotnet-install.ps1"
    Invoke-WebRequest -Uri "https://dot.net/v1/dotnet-install.ps1" -OutFile $installer
    & $installer -Channel 8.0 -InstallDir "C:\Program Files\dotnet"
    $env:PATH = "C:\Program Files\dotnet;$env:PATH"
    [Environment]::SetEnvironmentVariable("PATH", "C:\Program Files\dotnet;$([Environment]::GetEnvironmentVariable('PATH','Machine'))", "Machine")
    Write-Host ".NET SDK 8 installed."
}

# --- WiX Toolset (.NET global tool) ---
Write-Step "Installing WiX Toolset"
$dotnetToolsPath = "$env:USERPROFILE\.dotnet\tools"
if ($env:PATH -notlike "*$dotnetToolsPath*") {
    $env:PATH = "$dotnetToolsPath;$env:PATH"
    [Environment]::SetEnvironmentVariable("PATH", "$dotnetToolsPath;$([Environment]::GetEnvironmentVariable('PATH','User'))", "User")
}
dotnet tool install --global wix 2>$null
& "$dotnetToolsPath\wix.exe" extension add WixToolset.UI.wixext/6.0.0 --global 2>$null
Write-Host "WiX installed."

# --- Inno Setup 6 ---
Write-Step "Installing Inno Setup 6"
$innoPath = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
if (Test-Path $innoPath) {
    Write-Host "Inno Setup 6 already installed, skipping."
} else {
    $installer = "$env:TEMP\innosetup.exe"
    Invoke-WebRequest -Uri "https://jrsoftware.org/download.php/is.exe" -OutFile $installer
    Start-Process -FilePath $installer -ArgumentList "/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART","/SP-" -Wait
    Write-Host "Inno Setup 6 installed."
}

# --- PowerShell 7 (pwsh) ---
Write-Step "Installing PowerShell 7"
if (Get-Command pwsh -ErrorAction SilentlyContinue) {
    Write-Host "PowerShell 7 already installed, skipping."
} else {
    $installer = "$env:TEMP\pwsh-install.msi"
    Invoke-WebRequest -Uri "https://github.com/PowerShell/PowerShell/releases/download/v7.4.7/PowerShell-7.4.7-win-x64.msi" -OutFile $installer
    Start-Process msiexec.exe -ArgumentList "/i",$installer,"/quiet","/norestart" -Wait
    $env:PATH = "C:\Program Files\PowerShell\7;$env:PATH"
    [Environment]::SetEnvironmentVariable("PATH", "C:\Program Files\PowerShell\7;$([Environment]::GetEnvironmentVariable('PATH','Machine'))", "Machine")
    Write-Host "PowerShell 7 installed."
}

# --- LLVM/Clang (needed by ring crate for ARM64 cross-compilation) ---
Write-Step "Installing LLVM/Clang"
if (Get-Command clang -ErrorAction SilentlyContinue) {
    Write-Host "LLVM/Clang already installed, skipping."
} else {
    $installer = "$env:TEMP\llvm-install.exe"
    Invoke-WebRequest -Uri "https://github.com/llvm/llvm-project/releases/download/llvmorg-18.1.8/LLVM-18.1.8-win64.exe" -OutFile $installer
    Start-Process -FilePath $installer -ArgumentList "/S" -Wait
    $env:PATH = "C:\Program Files\LLVM\bin;$env:PATH"
    [Environment]::SetEnvironmentVariable("PATH", "C:\Program Files\LLVM\bin;$([Environment]::GetEnvironmentVariable('PATH','Machine'))", "Machine")
    Write-Host "LLVM/Clang installed."
}

# --- Add Git bash to system PATH (required by GitHub Actions) ---
Write-Step "Adding Git bash to system PATH"
$gitBashPath = "C:\Program Files\Git\bin"
if (Test-Path "$gitBashPath\bash.exe") {
    $machinePath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
    if ($machinePath -notlike "*$gitBashPath*") {
        [Environment]::SetEnvironmentVariable("PATH", "$gitBashPath;$machinePath", "Machine")
        $env:PATH = "$gitBashPath;$env:PATH"
        Write-Host "Added $gitBashPath to system PATH."
    } else {
        Write-Host "Git bash already in PATH, skipping."
    }
} else {
    Write-Host "WARNING: Git bash not found at $gitBashPath. Actions using 'shell: bash' will fail." -ForegroundColor Red
}

# --- Summary ---
Write-Step "Setup complete! Verifying installations:"
Write-Host "  Rust:       $(rustc --version 2>$null)"
Write-Host "  Cargo:      $(cargo --version 2>$null)"
Write-Host "  Git:        $(git --version 2>$null)"
Write-Host "  .NET:       $(dotnet --version 2>$null)"
Write-Host "  7-Zip:      $(if (Get-Command 7z -ErrorAction SilentlyContinue) { '✓ Installed' } else { '✗ NOT FOUND' })"
Write-Host "  Inno Setup: $(if (Test-Path $innoPath) { '✓ Installed' } else { '✗ NOT FOUND' })"
Write-Host "  MSVC:       $(if (!$needsVS) { '✓ Installed' } else { 'Check manually' })"
Write-Host "`nYou may need to restart your shell for PATH changes to take effect." -ForegroundColor Yellow
