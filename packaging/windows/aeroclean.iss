; Inno Setup script for the AeroClean installer (built by CI on Windows):
;   iscc /DVersion=1.3.0 /DSource=dist\AeroClean.exe packaging\windows\aeroclean.iss
; Installs for the current user only, so it needs no administrator rights.

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Source
  #define Source "..\..\target\dist\aero-cleaner.exe"
#endif

[Setup]
AppId={{6B3E3C1A-7A43-4E0B-9C61-AE20C1EA0001}
AppName=AeroClean
AppVersion={#Version}
AppPublisher=AeroClean
AppPublisherURL=https://github.com/aizenprivai/aero-cleaner
AppSupportURL=https://github.com/aizenprivai/aero-cleaner/issues
DefaultDirName={localappdata}\Programs\AeroClean
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
OutputBaseFilename=AeroClean-{#Version}-windows-setup
SetupIconFile=..\..\assets\aero-cleaner.ico
UninstallDisplayIcon={app}\AeroClean.exe
UninstallDisplayName=AeroClean
LicenseFile=..\..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Source}"; DestDir: "{app}"; DestName: "AeroClean.exe"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
; Same path the app uses for its own shortcut, so there's never a duplicate.
Name: "{userprograms}\AeroClean"; Filename: "{app}\AeroClean.exe"; Comment: "Clean caches, temp files and junk"
Name: "{userdesktop}\AeroClean"; Filename: "{app}\AeroClean.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\AeroClean.exe"; Description: "{cm:LaunchProgram,AeroClean}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Remove the weekly clean and low-disk alert, if they were turned on.
Filename: "schtasks.exe"; Parameters: "/Delete /F /TN ""AeroClean\aero-cleaner-clean"""; Flags: runhidden; RunOnceId: "DelClean"
Filename: "schtasks.exe"; Parameters: "/Delete /F /TN ""AeroClean\aero-cleaner-disk"""; Flags: runhidden; RunOnceId: "DelDisk"
