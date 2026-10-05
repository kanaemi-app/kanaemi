# Build Kanaemi and package it as a Windows Installer package into
# target\package. Needs the WiX toolset (the `wix` .NET tool).
#
# The package is for this machine's architecture, and carries the x86 DLL
# that 32-bit applications load.
$ErrorActionPreference = 'Stop'

$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$manifest = Join-Path $root 'Cargo.toml'
$release = Join-Path $root 'target\release'
$x86 = 'i686-pc-windows-msvc'
$out = Join-Path $root 'target\package'

function Invoke-Checked {
    & $args[0] @($args | Select-Object -Skip 1)
    if ($LASTEXITCODE -ne 0) { throw "$($args[0]) failed with $LASTEXITCODE" }
}

Invoke-Checked cargo build --release -p kanaemi-windows -p kanaemi-settings --bins --lib --manifest-path $manifest
Invoke-Checked rustup target add $x86
Invoke-Checked cargo build --release -p kanaemi-windows --lib --target $x86 --manifest-path $manifest

# The version kanaemi_core::VERSION reports, as the settings app prints it.
$version = (& (Join-Path $release 'kanaemi-settings.exe') --version).Trim()
# Windows Installer takes only numbers, so a build past a tag
# (0.1.0-3-gabc1234) carries the tag's version.
$packageVersion = ($version -split '-')[0]
$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { 'x64' }
    'ARM64' { 'arm64' }
    default { throw "no package architecture for $env:PROCESSOR_ARCHITECTURE" }
}

New-Item -ItemType Directory -Force $out | Out-Null
Invoke-Checked wix build (Join-Path $PSScriptRoot 'package\kanaemi.wxs') `
    -arch $arch `
    -d "Version=$packageVersion" `
    -d "Release=$release" `
    -d "ReleaseX86=$(Join-Path $root "target\$x86\release")" `
    -o (Join-Path $out "Kanaemi-$version-$arch.msi")
