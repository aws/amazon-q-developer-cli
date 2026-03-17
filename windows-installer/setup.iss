; Kiro CLI Installer Script (Inno Setup)
; Produces a setup.exe with license screen, custom install directory,
; PATH registration, and upgrade support.

#ifndef Version
  #define Version "0.0.0"
#endif

#ifndef MyAppExeSource
  #define MyAppExeSource "..\target\x86_64-pc-windows-msvc\release\chat_cli.exe"
#endif

[Setup]
AppName=Kiro CLI
AppVersion={#Version}
AppPublisher=Kiro
DefaultDirName={autopf}\Kiro-Cli
DefaultGroupName=Kiro CLI
OutputDir=output
OutputBaseFilename=kiro-cli-setup-{#Version}
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=license.txt
ChangesEnvironment=yes
PrivilegesRequired=admin
WizardStyle=modern
SetupIconFile=compiler:SetupClassicIcon.ico
UninstallDisplayIcon={app}\kiro-cli.exe

[Files]
Source: "{#MyAppExeSource}"; DestDir: "{app}"; DestName: "kiro-cli.exe"; Flags: ignoreversion

[Registry]
Root: HKLM; Subkey: "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"; \
  ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; \
  Check: NeedsAddPath(ExpandConstant('{app}'))

[UninstallDelete]
Type: dirifempty; Name: "{app}"

[Code]
function NeedsAddPath(Param: string): boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_LOCAL_MACHINE,
    'SYSTEM\CurrentControlSet\Control\Session Manager\Environment',
    'Path', OrigPath)
  then begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Path: string;
  AppDir: string;
  P: Integer;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    if RegQueryStringValue(HKEY_LOCAL_MACHINE,
      'SYSTEM\CurrentControlSet\Control\Session Manager\Environment',
      'Path', Path) then
    begin
      AppDir := ExpandConstant('{app}');
      P := Pos(';' + Uppercase(AppDir), ';' + Uppercase(Path));
      if P > 0 then
      begin
        Delete(Path, P - 1, Length(AppDir) + 1);
        RegWriteStringValue(HKEY_LOCAL_MACHINE,
          'SYSTEM\CurrentControlSet\Control\Session Manager\Environment',
          'Path', Path);
      end;
    end;
  end;
end;
