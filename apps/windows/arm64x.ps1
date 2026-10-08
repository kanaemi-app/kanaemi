# Make the DLLs of the text service for ARM64 Windows in the given folder:
# kanaemi_arm64.dll for ARM64 processes, kanaemi_x64.dll for x64 ones, and
# kanaemi.dll, the ARM64X DLL to register, which holds no code and hands each
# kind of process the DLL built for it. The registry has one 64-bit side for
# both kinds, and neither can load the other's DLL.
#
# Run on ARM64 after the release build for this machine (which builds
# kanaemi-load too). Needs the MSVC ARM64/ARM64EC build tools.
param([Parameter(Mandatory)] [string] $Out)
$ErrorActionPreference = 'Stop'

$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$manifest = Join-Path $root 'Cargo.toml'
$x64 = 'x86_64-pc-windows-msvc'
$work = Join-Path $root 'target\arm64x-work'

function Invoke-Checked {
    & $args[0] @($args | Select-Object -Skip 1)
    if ($LASTEXITCODE -ne 0) { throw "$($args[0]) failed with $LASTEXITCODE" }
}

Invoke-Checked rustup target add $x64
Invoke-Checked cargo build --release -p kanaemi-windows --lib --bin kanaemi-load --target $x64 --manifest-path $manifest

# lib reads an import library left from an earlier run instead of replacing it.
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Out, $work | Out-Null
Copy-Item -Force (Join-Path $root 'target\release\kanaemi.dll') (Join-Path $Out 'kanaemi_arm64.dll')
Copy-Item -Force (Join-Path $root "target\$x64\release\kanaemi.dll") (Join-Path $Out 'kanaemi_x64.dll')

# The compiler and linker that make ARM64X files, from the latest Visual
# Studio that has them.
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.ARM64 -property installationPath
if (-not $vs) { throw 'no Visual Studio with the MSVC ARM64/ARM64EC build tools' }
Import-Module (Join-Path $vs 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments '-arch=arm64 -host_arch=arm64' | Out-Null

# The forwarder exports what the DLL does, each forwarded by name to the DLL
# for one kind of process; kanaemi.def lists them. The linker resolves each
# forwarded name through an import library of the DLL it names, made here
# from the same list since each DLL's own one is named kanaemi.dll.
$exports = Get-Content (Join-Path $PSScriptRoot 'kanaemi.def') |
    ForEach-Object { ($_.Trim() -split '\s+')[0] } |
    Where-Object { $_ -and $_ -ne 'EXPORTS' }
function New-Forwarding($target, $machine) {
    $imports = Join-Path $work "$target.imports.def"
    @("LIBRARY $target", 'EXPORTS') + ($exports | ForEach-Object { "    $_" }) | Set-Content -Encoding ascii $imports
    $lib = Join-Path $work "$target.lib"
    Invoke-Checked lib /nologo "/machine:$machine" "/def:$imports" "/out:$lib"
    $def = Join-Path $work "$target.def"
    @('EXPORTS') + ($exports | ForEach-Object { "    $_ = $target.$_ PRIVATE" }) | Set-Content -Encoding ascii $def
    @{ Def = $def; Lib = $lib }
}
$toArm64 = New-Forwarding 'kanaemi_arm64' 'arm64'
$toX64 = New-Forwarding 'kanaemi_x64' 'x64'

# The steps Microsoft gives for an ARM64X pure forwarder: the linker wants an
# object for each side, empty since the forwarder holds no code. /Zl keeps
# the C runtime out of them, so nothing else needs linking.
$empty = Join-Path $work 'empty.c'
Set-Content -Encoding ascii $empty ''
$emptyArm64 = Join-Path $work 'empty_arm64.obj'
$emptyX64 = Join-Path $work 'empty_x64.obj'
Invoke-Checked cl /nologo /c /Zl "/Fo$emptyArm64" $empty
Invoke-Checked cl /nologo /c /Zl /arm64EC "/Fo$emptyX64" $empty
$forwarder = Join-Path $Out 'kanaemi.dll'
Invoke-Checked link /nologo /dll /noentry /machine:arm64x `
    "/defArm64Native:$($toArm64.Def)" "/def:$($toX64.Def)" `
    $emptyArm64 $emptyX64 $toArm64.Lib $toX64.Lib `
    "/implib:$(Join-Path $work 'kanaemi.lib')" "/out:$forwarder"

# Each kind of process loads the forwarder and must reach its own DLL.
$checks = @(
    @{ Load = Join-Path $root 'target\release\kanaemi-load.exe'; Dll = 'kanaemi_arm64.dll' },
    @{ Load = Join-Path $root "target\$x64\release\kanaemi-load.exe"; Dll = 'kanaemi_x64.dll' }
)
foreach ($check in $checks) {
    $lines = @(& $check.Load $forwarder @exports)
    if ($LASTEXITCODE -ne 0) { throw "$($check.Load) failed with $LASTEXITCODE" }
    if ($lines.Count -ne $exports.Count) { throw "$($check.Load) reached $($lines.Count) of the exports" }
    foreach ($line in $lines) {
        $name, $file = $line -split ' ', 2
        if ((Split-Path -Leaf $file) -ne $check.Dll) { throw "$name runs in $file, not $($check.Dll)" }
    }
}
