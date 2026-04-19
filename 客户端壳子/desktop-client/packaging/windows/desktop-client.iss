#define MyAppName "Sub2API Desktop Client"
#define MyAppPublisher "Sub2API"
#define MyAppExeName "sub2api-desktop.exe"

#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

#ifndef MySourceExe
  #error "MySourceExe is required"
#endif

#ifndef MyOutputDir
  #error "MyOutputDir is required"
#endif

#ifndef MyRepoRoot
  #error "MyRepoRoot is required"
#endif

#ifndef MyLicenseFile
  #define MyLicenseFile "{#MyRepoRoot}\LICENSE"
#endif

[Setup]
AppId={{8C8F44A6-247B-4947-A43C-8D577D6D9D0E}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL=https://github.com/Wei-Shaw/sub2api
AppSupportURL=https://github.com/Wei-Shaw/sub2api
DefaultGroupName={#MyAppName}
UninstallDisplayIcon={app}\{#MyAppExeName}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline
DefaultDirName={localappdata}\Programs\Sub2API Desktop Client
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=no
DisableProgramGroupPage=yes
LicenseFile={#MyLicenseFile}
SetupIconFile={#MyRepoRoot}\desktop-client\assets\app.ico
OutputDir={#MyOutputDir}
OutputBaseFilename=Sub2API-Desktop-Setup-{#MyAppVersion}

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加图标:"

[Files]
Source: "{#MySourceExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MyLicenseFile}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "启动 {#MyAppName}"; Flags: nowait postinstall skipifsilent
