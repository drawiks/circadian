#ifndef AppVersion
  #define AppVersion "0.3.0"
#endif

#ifndef BinDir
  #define BinDir "..\target\release"
#endif

#define AppName "Circadian"
#define AppPublisher "drawiks"
#define AppUrl "https://github.com/drawiks/circadian"

[Setup]
AppId={{FBDD3FDB-AA64-4222-A8F2-D925CEB3EAD3}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
DefaultDirName={localappdata}\Programs\Circadian
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=Circadian-Setup-{#AppVersion}
SetupIconFile=..\crates\daemon\assets\circadian.ico
UninstallDisplayIcon={app}\assets\circadian.ico
UninstallDisplayName={#AppName} {#AppVersion}
LicenseFile=..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.18362
CloseApplications=yes
RestartApplications=no
ShowLanguageDialog=auto
SetupLogging=yes
VersionInfoVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
VersionInfoDescription={#AppName} sleep tracker installer
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#BinDir}\circadian-app.exe"; DestDir: "{app}"; DestName: "circadian.exe"; Flags: ignoreversion
Source: "{#BinDir}\circadian-daemon.exe"; DestDir: "{app}"; DestName: "circadiand.exe"; Flags: ignoreversion
Source: "..\crates\daemon\assets\circadian.ico"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.ru.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\circadian.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\circadian.ico"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\circadian.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\circadian.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\circadian.exe"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[CustomMessages]
russian.RemoveDataPrompt=Удалить историю сна и настройки Circadian? Это действие необратимо.
english.RemoveDataPrompt=Remove Circadian sleep history and settings? This cannot be undone.

[Code]
procedure StopCircadian;
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM circadian.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM circadiand.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopCircadian;
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    StopCircadian;
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Circadian');
  end;
  if CurUninstallStep = usPostUninstall then
  begin
    if not UninstallSilent then
    begin
      if MsgBox(ExpandConstant('{cm:RemoveDataPrompt}'), mbConfirmation, MB_YESNO) = IDYES then
        DelTree(ExpandConstant('{localappdata}\Circadian'), True, True, True);
    end;
  end;
end;
