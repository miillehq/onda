param([switch]$Release, [switch]$WithVulkan)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'toolchain.ps1')
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repositoryDirectory
try {
    $buildMode = if ($Release) { 'release' } else { 'debug' }
    [string[]]$releaseArguments = @()
    if ($Release) { $releaseArguments = @('--release') }
    cargo build --locked -p eagle-input --bin eagle-input @releaseArguments
    if ($LASTEXITCODE -ne 0) { throw 'Shortcut helper build failed.' }
    cargo build --locked -p eagle-asr --features native-asr --bin eagle-asr-worker @releaseArguments
    if ($LASTEXITCODE -ne 0) { throw 'CPU ASR worker build failed.' }
    $binaryDirectory = Join-Path $repositoryDirectory 'src-tauri/binaries'
    [System.IO.Directory]::CreateDirectory($binaryDirectory) | Out-Null
    foreach ($binaryName in @('eagle-input','eagle-asr-worker')) {
        Copy-Item -LiteralPath (Join-Path $repositoryDirectory "target/$buildMode/$binaryName.exe") -Destination (Join-Path $binaryDirectory "$binaryName-x86_64-pc-windows-msvc.exe") -Force
    }
    if ($WithVulkan) {
        $previousCmakeGenerator=$env:CMAKE_GENERATOR
        try {
        . (Join-Path $PSScriptRoot 'vulkan-toolchain.ps1')
        if (-not $env:VULKAN_SDK) { throw 'Set VULKAN_SDK to an installed Vulkan SDK for the optional accelerator build.' }
        $vulkanTargetDirectory = if ($env:EAGLE_VULKAN_TARGET_DIR) { $env:EAGLE_VULKAN_TARGET_DIR } else { Join-Path ([System.IO.Path]::GetPathRoot($repositoryDirectory)) 'eagle-vulkan-target' }
        if (-not [System.IO.Path]::IsPathRooted($vulkanTargetDirectory) -or $vulkanTargetDirectory.Length -gt 60) { throw 'EAGLE_VULKAN_TARGET_DIR must be an absolute short path (60 characters maximum) for MSBuild shader compilation.' }
        cargo build --locked -p eagle-asr --features vulkan --bin eagle-asr-worker --target-dir $vulkanTargetDirectory @releaseArguments
        if ($LASTEXITCODE -ne 0) { throw 'Vulkan worker build failed; CPU artifacts are still usable.' }
        Copy-Item -LiteralPath (Join-Path $vulkanTargetDirectory "$buildMode/eagle-asr-worker.exe") -Destination (Join-Path $binaryDirectory 'eagle-asr-vulkan-worker-x86_64-pc-windows-msvc.exe') -Force
        Copy-Item -LiteralPath (Join-Path $vulkanTargetDirectory "$buildMode/eagle-asr-worker.exe") -Destination (Join-Path $repositoryDirectory "target/$buildMode/eagle-asr-vulkan-worker.exe") -Force
        } finally { $env:CMAKE_GENERATOR=$previousCmakeGenerator }
    }
} finally { Pop-Location }
