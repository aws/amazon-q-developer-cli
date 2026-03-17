@echo off
echo Building Kiro CLI Installer...

REM Check if WiX is installed
where wix >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo WiX Toolset not found. Install it with:
    echo   dotnet tool install --global wix
    exit /b 1
)

REM Add UI extension compatible with WiX v6
echo Adding WiX UI extension...
wix extension add WixToolset.UI.wixext/6.0.0 --global

REM Build the MSI with UI extension for installer dialogs
wix build Product.wxs -arch x64 -ext WixToolset.UI.wixext -o KiroCli-1.25.0.msi

if %ERRORLEVEL% EQU 0 (
    echo.
    echo Success! Created KiroCli-1.25.0.msi
    echo.
    echo To install: msiexec /i KiroCli-1.25.0.msi
    echo To uninstall: msiexec /x KiroCli-1.25.0.msi
) else (
    echo Build failed.
)
