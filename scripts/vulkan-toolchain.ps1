param([string] $VulkanSdkDirectory = $env:VULKAN_SDK)

$ErrorActionPreference = 'Stop'
if (-not $VulkanSdkDirectory -or -not (Test-Path -LiteralPath (Join-Path $VulkanSdkDirectory 'Bin\glslc.exe') -PathType Leaf)) {
    throw 'Optional Vulkan builds require VULKAN_SDK with Bin\glslc.exe, Include\vulkan and Lib\vulkan-1.lib.'
}
foreach ($requiredSdkFile in @('Include\vulkan\vulkan.hpp', 'Lib\vulkan-1.lib')) {
    if (-not (Test-Path -LiteralPath (Join-Path $VulkanSdkDirectory $requiredSdkFile) -PathType Leaf)) { throw "Vulkan SDK is missing $requiredSdkFile." }
}

$visualStudioLocator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $visualStudioLocator -PathType Leaf)) { throw 'Visual Studio Build Tools locator is missing.' }
$visualStudioDirectory = & $visualStudioLocator -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $visualStudioDirectory) { throw 'MSVC x64 build tools are required for the Vulkan worker.' }
$developerCommand = Join-Path $visualStudioDirectory 'Common7\Tools\VsDevCmd.bat'
$ninjaDirectory = Join-Path $visualStudioDirectory 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja'
if (-not (Test-Path -LiteralPath $developerCommand -PathType Leaf)) { throw 'MSVC developer environment command is missing.' }
if (-not (Test-Path -LiteralPath (Join-Path $ninjaDirectory 'ninja.exe') -PathType Leaf)) { throw 'Install the Visual Studio CMake/Ninja build-tools component.' }
if ($developerCommand.IndexOfAny([char[]]'"&|<>') -ge 0) { throw 'MSVC developer command path cannot be represented safely.' }

# Only a verified local developer command is executed; its environment output is captured and never printed.
$developerCommandText = 'call "' + $developerCommand + '" -no_logo -arch=x64 -host_arch=x64 >nul && set'
$developerEnvironmentLines = & $env:ComSpec /d /s /c $developerCommandText
if ($LASTEXITCODE -ne 0) { throw 'MSVC developer environment initialization failed.' }
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
$env:PATH = "$ninjaDirectory;$env:PATH"
$env:VULKAN_SDK = (Resolve-Path -LiteralPath $VulkanSdkDirectory).Path
$env:CMAKE_GENERATOR = 'Ninja'
