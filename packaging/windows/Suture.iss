#define MyAppName "Suture"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "Erik0318"
#define MyAppExeName "Suture.exe"

[Setup]
AppId={{5F9679B5-9A12-4F4F-93D6-C8670972A972}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\Programs\Suture
DisableProgramGroupPage=yes
OutputDir=..\..\dist-windows
OutputBaseFilename=Suture1.0.0-Windows-x86_64-Setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Files]
Source: "..\..\dist-windows\app\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Suture"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\Suture"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch Suture"; Flags: nowait postinstall skipifsilent
