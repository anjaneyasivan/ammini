; Ammini Windows installer (Inno Setup 6).
;
; Built by .github/workflows/build-windows-installer.yml (staging) — it is NOT yet
; part of the release pipeline. `MyAppVersion` is passed on the ISCC command line
; (/DMyAppVersion=<cargo version>); the fallback below is only for local runs.
;
; Source paths are relative to this script's directory (Inno's default SourceDir),
; so `..\dist\Ammini-x64` is the staged folder the Windows build produces.

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

[Setup]
; Fixed AppId: MUST stay the same across releases so Inno treats a newer build as
; an upgrade (in-place update, single entry in Add/Remove Programs) rather than a
; second side-by-side install. Generate a replacement with Tools ▸ Generate GUID.
AppId={{94CEFD6D-E0A1-4474-853F-E1D3AF4A9E93}
AppName=Ammini
AppVersion={#MyAppVersion}
AppPublisher=Ammini
; Fixed machine-wide install under C:\Program Files; requires admin (UAC prompt).
DefaultDirName={commonpf64}\Ammini
DisableDirPage=yes
DisableProgramGroupPage=yes
DefaultGroupName=Ammini
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=output
OutputBaseFilename=Ammini-{#MyAppVersion}-Setup
Compression=lzma2
SolidCompression=yes
SetupIconFile=..\assets\ammini.ico
UninstallDisplayIcon={app}\ammini.exe
; Let the Restart Manager close a running Ammini during an upgrade.
CloseApplications=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
; ammini.exe plus the libmpv runtime DLLs staged by the build (libmpv-2.dll,
; lua51.dll, vulkan-1.dll, …). `ignoreversion` keeps each release's own copies.
Source: "..\dist\Ammini-x64\*"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Ammini"; Filename: "{app}\ammini.exe"
Name: "{autodesktop}\Ammini"; Filename: "{app}\ammini.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"

[Run]
Filename: "{app}\ammini.exe"; Description: "Launch Ammini"; Flags: nowait postinstall skipifsilent
