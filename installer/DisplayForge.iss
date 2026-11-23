; DisplayForge installer (Inno Setup 6). Per-user: no administrator rights needed.

#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#ifndef TargetDir
  #define TargetDir "..\target"
#endif

[Setup]
AppId={{F250CAB3-BA9C-434B-8FCE-151D5CD5622A}
AppName=DisplayForge
AppVersion={#AppVersion}
AppVerName=DisplayForge {#AppVersion}
AppPublisher=Zheer Khalid
AppPublisherURL=https://zheer.dev
AppSupportURL=https://github.com/zheerkhalid/displayforge
AppUpdatesURL=https://github.com/zheerkhalid/displayforge/releases
DefaultDirName={autopf}\DisplayForge
DefaultGroupName=DisplayForge
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=DisplayForge-Setup
SetupIconFile={#TargetDir}\displayforge.ico
UninstallDisplayIcon={app}\DisplayForge.exe
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
LicenseFile=..\LICENSE
VersionInfoVersion={#AppVersion}
VersionInfoCompany=Zheer Khalid (zheer.dev)
VersionInfoDescription=DisplayForge Setup

[Tasks]
Name: "startup"; Description: "Start DisplayForge when I sign in to Windows (it runs in the notification area)"; GroupDescription: "Start-up:"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#TargetDir}\release\DisplayForge.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{group}\DisplayForge"; Filename: "{app}\DisplayForge.exe"
Name: "{group}\Uninstall DisplayForge"; Filename: "{uninstallexe}"
Name: "{autodesktop}\DisplayForge"; Filename: "{app}\DisplayForge.exe"; Tasks: desktopicon

[Registry]
; The same value the "Start with Windows" switch in Settings writes.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "DisplayForge"; ValueData: """{app}\DisplayForge.exe"" --background"; Tasks: startup; Flags: uninsdeletevalue

[Run]
Filename: "{app}\DisplayForge.exe"; Description: "{cm:LaunchProgram,DisplayForge}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Ask DisplayForge to restore the screens' colours and exit; then make sure it is gone.
Filename: "{app}\DisplayForge.exe"; Parameters: "--quit"; Flags: runhidden; RunOnceId: "QuitDisplayForge"
Filename: "{cmd}"; Parameters: "/C timeout /T 2 /NOBREAK >NUL & taskkill /IM DisplayForge.exe /F"; Flags: runhidden; RunOnceId: "KillDisplayForge"

[Code]
// Before files are replaced on an upgrade: ask a running DisplayForge to exit.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
  Exe: String;
begin
  Result := '';
  Exe := ExpandConstant('{app}\DisplayForge.exe');
  if FileExists(Exe) then
  begin
    Exec(Exe, '--quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Sleep(1500);
  end;
end;

// Settings in %LOCALAPPDATA%\DisplayForge are kept on uninstall unless the
// user says otherwise. The startup value is always removed.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DisplayForge');
    DataDir := ExpandConstant('{localappdata}\DisplayForge');
    if DirExists(DataDir) and not UninstallSilent then
      if MsgBox('Also delete your DisplayForge settings (presets, schedule, app profiles)?' + #13#10 + DataDir,
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(DataDir, True, True, True);
  end;
end;
