#ifndef AppVersion
  #define AppVersion "3.0.1"
#endif
#ifndef BinaryPath
  #define BinaryPath "..\target\release\polban-absen-gui.exe"
#endif
#ifndef IconPath
  #define IconPath "..\assets\app-icon.ico"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{B9E6D4A1-8C3F-4F2A-9A7D-3E5F2C1B8D04}
AppName=Polban Presensi
AppVersion={#AppVersion}
AppVerName=Polban Presensi {#AppVersion}
AppPublisher=Polban Presensi
AppPublisherURL=https://github.com/raditted/Polban-Auto-Absen-rs
AppSupportURL=https://github.com/raditted/Polban-Auto-Absen-rs/issues
DefaultDirName={autopf}\Polban Presensi
DefaultGroupName=Polban Presensi
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename=Polban-Presensi-Setup-Windows-x64
SetupIconFile={#IconPath}
UninstallDisplayIcon={app}\Polban Presensi.exe
UninstallDisplayName=Polban Presensi
LicenseFile=..\LICENSE
CloseApplications=yes
RestartApplications=no
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Buat shortcut di Desktop"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#BinaryPath}"; DestDir: "{app}"; DestName: "Polban Presensi.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Polban Presensi"; Filename: "{app}\Polban Presensi.exe"
Name: "{autodesktop}\Polban Presensi"; Filename: "{app}\Polban Presensi.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Polban Presensi.exe"; Description: "{cm:LaunchProgram,Polban Presensi}"; Flags: nowait postinstall skipifsilent
