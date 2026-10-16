#ifndef AppVersion
  #error AppVersion required
#endif
[Setup]
AppId={{CAB20984-0875-4CAB-BEAB-B499E345764F}
AppName=Synapse
AppVersion={#AppVersion}
AppPublisher=Wess Cope
AppPublisherURL=https://github.com/wess/synapse
DefaultDirName={localappdata}\Programs\Synapse
DefaultGroupName=Synapse
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir={#Output}
OutputBaseFilename=synapse.{#AppVersion}.windows.x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\..\crates\synapse\assets\icon.ico
UninstallDisplayIcon={app}\synapse.ico
CloseApplications=yes
ChangesEnvironment=yes

[Files]
Source: "{#Stage}\synapsedesktop.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\synapse.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "license.txt"; Flags: ignoreversion
Source: "..\..\pi\package.json"; DestDir: "{app}\pi"; Flags: ignoreversion
Source: "..\..\pi\extensions\*"; DestDir: "{app}\pi\extensions"; Flags: ignoreversion recursesubdirs createallsubdirs; Excludes: "._*,.DS_Store"

Source: "..\..\crates\synapse\assets\icon.ico"; DestDir: "{app}"; DestName: "synapse.ico"; Flags: ignoreversion

[Icons]
Name: "{group}\Synapse"; Filename: "{app}\synapsedesktop.exe"; IconFilename: "{app}\synapse.ico"
Name: "{autodesktop}\Synapse"; Filename: "{app}\synapsedesktop.exe"; IconFilename: "{app}\synapse.ico"; Tasks: desktopicon

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked
Name: path; Description: "Add the Synapse CLI to your user PATH"

[Run]
Filename: "{app}\synapsedesktop.exe"; Description: "Open Synapse"; Flags: nowait postinstall skipifsilent

[Code]
const EnvKey = 'Environment';
procedure CurStepChanged(CurStep: TSetupStep);
var Current, Bin: String;
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('path') then begin
    Bin := ExpandConstant('{app}\bin');
    RegQueryStringValue(HKCU, EnvKey, 'Path', Current);
    if Pos(';' + Lowercase(Bin) + ';', ';' + Lowercase(Current) + ';') = 0 then begin
      if (Current <> '') and (Copy(Current, Length(Current), 1) <> ';') then Current := Current + ';';
      RegWriteExpandStringValue(HKCU, EnvKey, 'Path', Current + Bin);
      RegWriteDWordValue(HKCU, 'Software\Synapse\Installer', 'AddedPath', 1);
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var Current, Updated, Entry, Bin: String; Added: Cardinal; At: Integer;
begin
  if (CurUninstallStep = usPostUninstall) and
    RegQueryDWordValue(HKCU, 'Software\Synapse\Installer', 'AddedPath', Added) and (Added = 1) then begin
    Bin := ExpandConstant('{app}\bin');
    RegQueryStringValue(HKCU, EnvKey, 'Path', Current);
    Updated := '';
    while Current <> '' do begin
      At := Pos(';', Current);
      if At = 0 then At := Length(Current) + 1;
      Entry := Copy(Current, 1, At - 1);
      Delete(Current, 1, At);
      if Lowercase(Entry) <> Lowercase(Bin) then begin
        if Updated <> '' then Updated := Updated + ';';
        Updated := Updated + Entry;
      end;
    end;
    RegWriteExpandStringValue(HKCU, EnvKey, 'Path', Updated);
    RegDeleteValue(HKCU, 'Software\Synapse\Installer', 'AddedPath');
  end;
end;
