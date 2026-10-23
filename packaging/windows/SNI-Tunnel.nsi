Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
Name "SNI Tunnel V2"
OutFile "..\..\releases\v2\windows\SNI-Tunnel-V2-Setup-x64.exe"
InstallDir "$PROGRAMFILES64\SNI Tunnel V2"
InstallDirRegKey HKLM "Software\SNI Tunnel V2" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
Icon "..\..\assets\icon.ico"
UninstallIcon "..\..\assets\icon.ico"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Function .onInit
 ${IfNot} ${RunningX64}
  MessageBox MB_ICONSTOP "This installer requires 64-bit Windows 10 or later."
  Abort
 ${EndIf}
 SetRegView 64
FunctionEnd
Section "SNI Tunnel (required)"
 SectionIn RO
 SetOutPath "$INSTDIR"
 File "..\..\target\release\sni-tunnel.exe"
 File "..\..\packaging\README-INSTALL.txt"
 File "..\..\THIRD-PARTY-NOTICES.md"
 File "install-tap.ps1"
 SetOutPath "$INSTDIR\core"
 File /x .sha256 "..\staging\windows\core\*"
 SetOutPath "$INSTDIR\compat"
 File "..\staging\windows\compat\*"
 SetOutPath "$INSTDIR\driver"
 File "..\staging\windows\driver\*"
 SetOutPath "$INSTDIR\licenses"
 File /r "..\licenses\*"
 SetOutPath "$INSTDIR"
 nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\install-tap.ps1"'
 Pop $0
 StrCmp $0 "0" driver_ok
 MessageBox MB_ICONEXCLAMATION "TAP driver setup needs attention. After installation, run install-tap.ps1 as Administrator. See README-INSTALL.txt."
 driver_ok:
 WriteUninstaller "$INSTDIR\Uninstall.exe"
 SetShellVarContext all
 CreateDirectory "$SMPROGRAMS\SNI Tunnel V2"
 CreateShortcut "$SMPROGRAMS\SNI Tunnel V2\SNI Tunnel.lnk" "$INSTDIR\sni-tunnel.exe"
 CreateShortcut "$SMPROGRAMS\SNI Tunnel V2\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
 CreateShortcut "$DESKTOP\SNI Tunnel V2.lnk" "$INSTDIR\sni-tunnel.exe"
 WriteRegStr HKLM "Software\SNI Tunnel V2" "InstallDir" "$INSTDIR"
 WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SNI Tunnel V2" "DisplayName" "SNI Tunnel V2"
 WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SNI Tunnel V2" "DisplayVersion" "2.1.0"
 WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SNI Tunnel V2" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
 WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SNI Tunnel V2" "DisplayIcon" "$INSTDIR\sni-tunnel.exe"
 WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SNI Tunnel V2" "NoModify" 1
 WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SNI Tunnel V2" "NoRepair" 1
SectionEnd
Section "Uninstall"
 SetRegView 64
 SetShellVarContext all
 Delete "$DESKTOP\SNI Tunnel V2.lnk"
 Delete "$SMPROGRAMS\SNI Tunnel V2\SNI Tunnel.lnk"
 Delete "$SMPROGRAMS\SNI Tunnel V2\Uninstall.lnk"
 RMDir "$SMPROGRAMS\SNI Tunnel V2"
 Delete "$INSTDIR\sni-tunnel.exe"
 Delete "$INSTDIR\README-INSTALL.txt"
 Delete "$INSTDIR\THIRD-PARTY-NOTICES.md"
 Delete "$INSTDIR\install-tap.ps1"
 RMDir /r "$INSTDIR\core"
 RMDir /r "$INSTDIR\compat"
 RMDir /r "$INSTDIR\driver"
 RMDir /r "$INSTDIR\licenses"
 Delete "$INSTDIR\Uninstall.exe"
 RMDir "$INSTDIR"
 DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SNI Tunnel V2"
 DeleteRegKey HKLM "Software\SNI Tunnel V2"
 ; User profiles and the shared TAP driver are retained.
SectionEnd
