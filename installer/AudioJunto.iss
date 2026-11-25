#define AppVersion "0.2.0"

[Setup]
AppId={{529C9BD8-367D-4B6A-A815-105BDB0F24DA}
AppName=AudioJunto
AppVersion={#AppVersion}
AppVerName=AudioJunto {#AppVersion}
AppPublisher=AudioJunto
DefaultDirName={localappdata}\Programs\AudioJunto
DefaultGroupName=AudioJunto
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
MinVersion=10.0
OutputDir=..\..
OutputBaseFilename=AudioJunto-Setup-{#AppVersion}
SetupIconFile=payload\AudioJunto.ico
UninstallDisplayIcon={app}\AudioJunto.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayName=AudioJunto
VersionInfoVersion=0.2.0.0
VersionInfoDescription=Instalador do AudioJunto
VersionInfoProductName=AudioJunto

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "desktopicon"; Description: "Criar um atalho na área de trabalho"; GroupDescription: "Atalhos:"; Flags: unchecked

[Files]
Source: "payload\AudioJunto.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "payload\AudioJunto.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.pt-BR.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.pt-BR.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\AudioJunto"; Filename: "{app}\AudioJunto.exe"; IconFilename: "{app}\AudioJunto.ico"
Name: "{userdesktop}\AudioJunto"; Filename: "{app}\AudioJunto.exe"; IconFilename: "{app}\AudioJunto.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\AudioJunto.exe"; Description: "Abrir AudioJunto"; Flags: nowait postinstall skipifsilent unchecked
