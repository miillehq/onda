# Windows on ARM: whisper.cpp refuses MSVC's own compiler for ARM64 ("use clang"), so the speech
# worker builds with clang-cl and Ninja inside the ARM64 developer environment. Dot-source it;
# prepare.ps1 does so on an ARM64 host and restores the environment afterwards.
$ErrorActionPreference = 'Stop'
$visualStudioLocator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $visualStudioLocator -PathType Leaf)) { throw 'Visual Studio Build Tools locator is missing.' }
$visualStudioDirectory = & $visualStudioLocator -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.ARM64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $visualStudioDirectory) { throw 'MSVC ARM64 build tools are required on Windows on ARM.' }
$developerCommand = Join-Path $visualStudioDirectory 'Common7\Tools\VsDevCmd.bat'
$ninjaDirectory = Join-Path $visualStudioDirectory 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja'
if (-not (Test-Path -LiteralPath $developerCommand -PathType Leaf)) { throw 'MSVC developer environment command is missing.' }
if (-not (Test-Path -LiteralPath (Join-Path $ninjaDirectory 'ninja.exe') -PathType Leaf)) { throw 'Install the Visual Studio CMake/Ninja build-tools component.' }
if ($developerCommand.IndexOfAny([char[]]'"&|<>') -ge 0) { throw 'MSVC developer command path cannot be represented safely.' }
$clang = if ($env:LIBCLANG_PATH) { Join-Path $env:LIBCLANG_PATH 'clang-cl.exe' } else { '' }
if (-not $clang -or -not (Test-Path -LiteralPath $clang -PathType Leaf)) { throw 'clang-cl.exe from LLVM is required next to libclang (run toolchain.ps1 first).' }

# Only the verified local developer command runs; its environment is captured, never printed.
$hostArchitecture = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
$developerCommandText = 'call "' + $developerCommand + '" -no_logo -arch=arm64 -host_arch=' + $hostArchitecture + ' >nul && set'
$developerEnvironmentLines = & $env:ComSpec /d /s /c $developerCommandText
if ($LASTEXITCODE -ne 0) { throw 'MSVC ARM64 developer environment initialization failed.' }
$allowedEnvironmentNames = @(
    'PATH', 'INCLUDE', 'LIB', 'LIBPATH', 'VCToolsInstallDir', 'VCINSTALLDIR',
    'WindowsSdkDir', 'WindowsSDKVersion', 'UniversalCRTSdkDir', 'UCRTVersion',
    'VisualStudioVersion', 'VSINSTALLDIR', 'VSCMD_ARG_TGT_ARCH', 'VSCMD_ARG_HOST_ARCH'
)
foreach ($environmentLine in $developerEnvironmentLines) {
    $separatorIndex = $environmentLine.IndexOf('=')
    if ($separatorIndex -le 0) { continue }
    $environmentName = $environmentLine.Substring(0, $separatorIndex)
    if ($allowedEnvironmentNames -notcontains $environmentName) { continue }
    [Environment]::SetEnvironmentVariable($environmentName, $environmentLine.Substring($separatorIndex + 1), 'Process')
}
$env:PATH = "$ninjaDirectory;$(Split-Path -Parent $clang);$env:PATH"
# Visual Studio generator settings from the environment (the runner image sets an instance) make
# CMake reject Ninja.
foreach ($name in @('CMAKE_GENERATOR_INSTANCE', 'CMAKE_GENERATOR_PLATFORM', 'CMAKE_GENERATOR_TOOLSET')) {
    [Environment]::SetEnvironmentVariable($name, $null, 'Process')
}
$env:CMAKE_GENERATOR = 'Ninja'
# whisper-rs-sys forwards CMAKE_* variables to CMake, which then compiles with clang-cl.
$env:CMAKE_C_COMPILER = $clang
$env:CMAKE_CXX_COMPILER = $clang
$env:CMAKE_ASM_COMPILER = $clang
# An x64 LLVM (emulated on ARM) would target x64 by default; name ARM64 explicitly.
$env:CMAKE_C_COMPILER_TARGET = 'aarch64-pc-windows-msvc'
$env:CMAKE_CXX_COMPILER_TARGET = 'aarch64-pc-windows-msvc'
# The cmake crate sets CMAKE_CXX_FLAGS itself, which drops CMake's default /EHsc; without it
# clang-cl compiles whisper.cpp with C++ exceptions disabled and rejects its try blocks.
$env:CXXFLAGS_aarch64_pc_windows_msvc = '/EHsc'
