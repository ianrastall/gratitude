; Inno Setup script for Show Player. Compile through build/build.ps1 so the
; version and the verified publish folder are supplied together.
;
; Per-user install: no administrator rights, files under
; %LOCALAPPDATA%\Programs\Show Player, a Start entry under the app's name
; and an uninstaller in Settings > Apps. The AppId and folder are those of
; 0.1 to 0.3, so this upgrades them in place. Saved positions and settings
; live in %LOCALAPPDATA%\show-player and are left alone by install,
; upgrade and uninstall.

#define AppName "Gratitude"
#define AppExe "ShowPlayer.exe"
#define AppUserModelId "IanRastall.ShowPlayer"
#ifndef AppVersion
  #error AppVersion must be supplied by build.ps1
#endif

[Setup]
AppId={{F2A40B58-E561-4D96-A143-63386A0842EF}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Ian Rastall
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 11 24H2, as the app and Jupiter.UI require.
MinVersion=10.0.26100
; The app holds this mutex while it runs, so Setup asks for it to be closed first.
AppMutex=ShowPlayer.SingleInstance
CloseApplications=no
RestartApplications=no
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExe}
SetupIconFile=..\src\ShowPlayer.App\Assets\AppIcon.ico
WizardStyle=modern
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
; Tell Explorer and Start to refresh icons after install and uninstall, so a
; reinstall does not leave a blank Start menu icon.
ChangesAssociations=yes
OutputDir=..\dist
OutputBaseFilename=Gratitude-{#AppVersion}-win-x64-setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[InstallDelete]
; Removed downloader runtime; existing user downloads stay untouched.
Type: files; Name: "{app}\tools\aria2c.exe"
Type: files; Name: "{autoprograms}\Show Player.lnk"
Type: files; Name: "{autodesktop}\Show Player.lnk"
; 0.1 to 0.3 shipped the C++ core; it is replaced by the Rust core.
Type: files; Name: "{app}\abcore.dll"
; A Jupiter folder in Start is never used; Start lists the app by name.
Type: files; Name: "{autoprograms}\Jupiter\{#AppName}.lnk"
Type: dirifempty; Name: "{autoprograms}\Jupiter"

[Files]
Source: "..\dist\ShowPlayer\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Excludes: "*.pdb"
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
; libmpv is LGPL: its license and the exact build ship with the app.
Source: "..\third_party\libmpv\LICENSE.LGPL"; DestDir: "{app}\licenses"; DestName: "libmpv-LICENSE.LGPL.txt"; Flags: ignoreversion
Source: "..\third_party\README.md"; DestDir: "{app}\licenses"; DestName: "libmpv-build.md"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; AppUserModelID: "{#AppUserModelId}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; AppUserModelID: "{#AppUserModelId}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
