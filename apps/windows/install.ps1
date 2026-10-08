# Build Kanaemi and install it into Program Files, registering the text service.
# Run from an elevated PowerShell: registering writes HKEY_LOCAL_MACHINE.
#
# The DLL goes into Program Files because processes in an AppContainer, such as
# the Start menu's search box, can read only there. A 32-bit application loads
# the x86 DLL beside it, registered by the 32-bit regsvr32. On ARM64 the DLLs
# are the ones arm64x.ps1 makes, so that x64 applications load it too.
$ErrorActionPreference = 'Stop'

$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$manifest = Join-Path $root 'Cargo.toml'
$dest = Join-Path $env:ProgramFiles 'Kanaemi'
$x86 = 'i686-pc-windows-msvc'

function Invoke-Checked {
    & $args[0] @($args | Select-Object -Skip 1)
    if ($LASTEXITCODE -ne 0) { throw "$($args[0]) failed with $LASTEXITCODE" }
}

Invoke-Checked cargo build --release -p kanaemi-windows -p kanaemi-settings --bins --lib --manifest-path $manifest
Invoke-Checked rustup target add $x86
Invoke-Checked cargo build --release -p kanaemi-windows --lib --target $x86 --manifest-path $manifest

# Applications keep the DLLs loaded, so they cannot be replaced, but they can
# be renamed: each application loads the new ones when it starts again.
# kanaemi.dll among them is the one registered.
function Install-Dll($from, $folder, $regsvr32) {
    New-Item -ItemType Directory -Force $folder | Out-Null
    Get-ChildItem $folder -Filter 'kanaemi*.old.*.dll' | Remove-Item -ErrorAction SilentlyContinue
    foreach ($file in $from) {
        $name = Split-Path -Leaf $file
        $installed = Join-Path $folder $name
        if (Test-Path $installed) {
            $old = [IO.Path]::GetFileNameWithoutExtension($name) + '.old.' + [guid]::NewGuid() + '.dll'
            Move-Item -Force $installed (Join-Path $folder $old)
        }
        Copy-Item $file $installed
    }
    $dll = Join-Path $folder 'kanaemi.dll'
    $registered = Start-Process $regsvr32 -ArgumentList '/s', "`"$dll`"" -Wait -PassThru
    if ($registered.ExitCode -ne 0) { throw "$regsvr32 failed with $($registered.ExitCode)" }
}

if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
    $arm64x = Join-Path $root 'target\arm64x'
    & (Join-Path $PSScriptRoot 'arm64x.ps1') $arm64x
    $dlls = 'kanaemi.dll', 'kanaemi_arm64.dll', 'kanaemi_x64.dll' | ForEach-Object { Join-Path $arm64x $_ }
} else {
    $dlls = Join-Path $root 'target\release\kanaemi.dll'
}
Install-Dll $dlls $dest 'regsvr32'
Install-Dll (Join-Path $root "target\$x86\release\kanaemi.dll") (Join-Path $dest 'x86') (Join-Path $env:windir 'SysWOW64\regsvr32.exe')

# Running programs keep their files: the new ones replace them once stopped.
Get-Process kanaemi-server, kanaemi-settings -ErrorAction SilentlyContinue | Stop-Process -Force
$settings = Join-Path $dest 'kanaemi-settings.exe'
Copy-Item -Force (Join-Path $root 'target\release\kanaemi-settings.exe') $settings
# The settings app opens from the Start menu too, not only from the input
# method's options in the Windows settings.
# Its Japanese name is spelled by code point: Windows PowerShell reads a
# script without a byte order mark in the system's code page.
$name = -join [char[]](0x304B, 0x306A, 0x3048, 0x307F, 0x8A2D, 0x5B9A)
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut(
    (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\$name.lnk"))
$shortcut.TargetPath = $settings
$shortcut.Save()
$server = Join-Path $dest 'kanaemi-server.exe'
Copy-Item -Force (Join-Path $root 'target\release\kanaemi-server.exe') $server

# The server writes for text services in AppContainers; it starts at every sign-in.
Set-ItemProperty 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Kanaemi -Value "`"$server`""
# Started by the shell, so it runs as the user signed in to the desktop and
# not elevated, whoever elevated this script: the pipe and folder it serves
# are that user's.
Start-Process explorer.exe -ArgumentList "`"$server`""
"installed $dest"
