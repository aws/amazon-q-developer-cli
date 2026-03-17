# Kiro CLI Installer

This folder contains installer configurations for Kiro CLI on Windows.

## Setup.exe (Inno Setup) — Recommended

The `setup.iss` script produces a standard Windows `setup.exe` installer with:
- Welcome screen and MIT license agreement
- Custom install directory selection (default: `C:\Program Files\Kiro-Cli\`)
- Automatic PATH registration (adds install dir to system PATH)
- Clean uninstall via Windows Settings > Apps
- Upgrade support (re-running the installer updates in place)

### Prerequisites

Install [Inno Setup 6](https://jrsoftware.org/isdl.php) (free).

### Building

1. Build the binary:
   ```
   cargo build --locked --package chat_cli --release --target x86_64-pc-windows-msvc
   ```

2. Build the installer:
   ```
   "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" /DVersion=1.26.2 windows-installer\setup.iss
   ```

3. Output: `windows-installer\output\kiro-cli-setup-1.26.2.exe`

### Installing

Run the setup exe. After installation, restart your terminal and run `kiro-cli` from anywhere.

### Silent Install

```
kiro-cli-setup-1.26.2.exe /SILENT /DIR="C:\MyCustomPath"
```

## MSI (WiX) — Legacy

The `Product.wxs` and `build.bat` files are an earlier WiX-based MSI approach.

### WiX Prerequisites

1. Install .NET SDK: https://dotnet.microsoft.com/download
2. Install WiX Toolset v6: `dotnet tool install --global wix`

### WiX Building

Run `build.bat` to generate `KiroCli-1.25.0.msi`.

## Files

- `setup.iss` - Inno Setup installer script (recommended)
- `license.txt` - License agreement shown during installation
- `Product.wxs` - WiX MSI configuration (legacy)
- `License.rtf` - RTF license for WiX installer
- `build.bat` - WiX MSI build script
