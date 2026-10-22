; Inno Setup script for the SNI Tunnel Windows x64 installer.
;
; Build:  ISCC.exe packaging\windows\SNI-Tunnel.iss /DAppVersion=0.4.0
;         (packaging\windows\build.ps1 does this and collects the result)
;
; The installer ships the application, the matching Xray-core build and the
; wintun driver. It never touches the user configuration, which lives in
; %LOCALAPPDATA%\SniTunnel and is not part of the package.

#define AppName "SNI Tunnel"
#define AppPublisher "SNI Tunnel Project"
#define AppExeName "sni-tunnel.exe"
#define AppId "{{7B0C4A2E-9E3D-4C1B-9C2E-5B1D0A7C41E2}"
#ifndef AppVersion
  #define AppVersion "0.4.0"
#endif
#ifndef SourceRoot
  #define SourceRoot "..\.."
#endif

[Setup]
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL=https://github.com/
AppSupportURL=https://github.com/
AppUpdatesURL=https://github.com/
DefaultDirName={autopf}\SNI Tunnel
DefaultGroupName=SNI Tunnel
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExeName}
SetupIconFile={#SourceRoot}\assets\icon.ico
OutputDir=..\..\dist
OutputBaseFilename=SNI-Tunnel-Setup-Windows-x64
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes
DisableDirPage=no
AllowNoIcons=yes
; The application runs as a normal user; only the TUN mode asks for
; elevation on its own. The installer therefore does not need admin rights.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
MinVersion=10.0
VersionInfoVersion={#AppVersion}
VersionInfoDescription={#AppName} installer
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
UninstallDisplaySize=32
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
english.CreateDesktopIcon=Create a &desktop shortcut
english.LaunchApp=Launch {#AppName}

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; --- the application -------------------------------------------------------
Source: "{#SourceRoot}\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; --- the networking core (Xray-core v26.3.27) -------------------------------
Source: "{#SourceRoot}\packaging\staging\windows\core\*"; DestDir: "{app}\core"; Flags: ignoreversion recursesubdirs createallsubdirs; Excludes: ".sha256"
; --- documentation and licences --------------------------------------------
Source: "{#SourceRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\packaging\staging\windows\core\LICENSE*"; DestDir: "{app}\core"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\{#AppExeName}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Only the application directory. The user configuration lives in
; %LOCALAPPDATA%\SniTunnel and is deliberately kept.
Type: filesandordirs; Name: "{app}\core"

[Code]
function InitializeSetup(): Boolean;
var
  CoreExe: String;
begin
  CoreExe := ExpandConstant('{#SourceRoot}\packaging\staging\windows\core\xray.exe');
  if not FileExists(CoreExe) then
  begin
    MsgBox('The Xray-core executable is missing:' + #13#10 + CoreExe + #13#10#13#10 +
           'Run packaging\windows\build.ps1 to stage the files before compiling.',
           mbError, MB_OK);
    Result := False;
    exit;
  end;
  Result := True;
end;
