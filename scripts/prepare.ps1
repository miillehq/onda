param([switch]$Release, [switch]$WithVulkan)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'toolchain.ps1')
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repositoryDirectory
try {
    $buildMode = if ($Release) { 'release' } else { 'debug' }
    [string[]]$releaseArguments = @()
    if ($Release) { $releaseArguments = @('--release') }
    cargo build --locked -p onda-input --bin onda-input @releaseArguments
    if ($LASTEXITCODE -ne 0) { throw 'Shortcut helper build failed.' }
    cargo build --locked -p onda-asr --features native-asr --bin onda-asr-worker @releaseArguments
    if ($LASTEXITCODE -ne 0) { throw 'CPU ASR worker build failed.' }
    # .cargo/config.toml keeps the default worker SSE2-only so it runs on any x86-64 CPU. This
    # second build targets AVX2/FMA/F16C CPUs (2013+) and is several times faster; Onda picks it at runtime.
    $avx2TargetDirectory = Join-Path $repositoryDirectory 'target/avx2'
    [string[]]$avx2Flags = @('GGML_AVX', 'GGML_AVX2', 'GGML_FMA', 'GGML_F16C', 'GGML_SSE42', 'GGML_BMI2') | ForEach-Object { '--config'; "env.$_.value='ON'" }
    cargo build --locked -p onda-asr --features native-asr --bin onda-asr-worker --target-dir $avx2TargetDirectory @avx2Flags @releaseArguments
    if ($LASTEXITCODE -ne 0) { throw 'AVX2 ASR worker build failed.' }
    $binaryDirectory = Join-Path $repositoryDirectory 'src-tauri/binaries'
    [System.IO.Directory]::CreateDirectory($binaryDirectory) | Out-Null
    foreach ($binaryName in @('onda-input','onda-asr-worker')) {
        Copy-Item -LiteralPath (Join-Path $repositoryDirectory "target/$buildMode/$binaryName.exe") -Destination (Join-Path $binaryDirectory "$binaryName-x86_64-pc-windows-msvc.exe") -Force
    }
    Copy-Item -LiteralPath (Join-Path $avx2TargetDirectory "$buildMode/onda-asr-worker.exe") -Destination (Join-Path $binaryDirectory 'onda-asr-avx2-worker-x86_64-pc-windows-msvc.exe') -Force
    Copy-Item -LiteralPath (Join-Path $avx2TargetDirectory "$buildMode/onda-asr-worker.exe") -Destination (Join-Path $repositoryDirectory "target/$buildMode/onda-asr-avx2-worker.exe") -Force
    if ($WithVulkan) {
        $previousCmakeGenerator=$env:CMAKE_GENERATOR
        # The developer environment prepends to PATH each time; restoring it lets the build run
        # repeatedly in one shell without hitting cmd's line-length limit.
        $previousToolEnvironment = @{}
        foreach ($name in @('PATH', 'INCLUDE', 'LIB', 'LIBPATH')) { $previousToolEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
        try {
        . (Join-Path $PSScriptRoot 'vulkan-toolchain.ps1')
        if (-not $env:VULKAN_SDK) { throw 'Set VULKAN_SDK to an installed Vulkan SDK for the optional accelerator build.' }
        $vulkanTargetDirectory = if ($env:ONDA_VULKAN_TARGET_DIR) { $env:ONDA_VULKAN_TARGET_DIR } else { Join-Path ([System.IO.Path]::GetPathRoot($repositoryDirectory)) 'onda-vulkan-target' }
        if (-not [System.IO.Path]::IsPathRooted($vulkanTargetDirectory) -or $vulkanTargetDirectory.Length -gt 60) { throw 'ONDA_VULKAN_TARGET_DIR must be an absolute short path (60 characters maximum) for MSBuild shader compilation.' }
        cargo build --locked -p onda-asr --features vulkan --bin onda-asr-worker --target-dir $vulkanTargetDirectory @releaseArguments
        if ($LASTEXITCODE -ne 0) { throw 'Vulkan worker build failed; CPU artifacts are still usable.' }
        Copy-Item -LiteralPath (Join-Path $vulkanTargetDirectory "$buildMode/onda-asr-worker.exe") -Destination (Join-Path $binaryDirectory 'onda-asr-vulkan-worker-x86_64-pc-windows-msvc.exe') -Force
        Copy-Item -LiteralPath (Join-Path $vulkanTargetDirectory "$buildMode/onda-asr-worker.exe") -Destination (Join-Path $repositoryDirectory "target/$buildMode/onda-asr-vulkan-worker.exe") -Force
        } finally {
            $env:CMAKE_GENERATOR=$previousCmakeGenerator
            foreach ($entry in $previousToolEnvironment.GetEnumerator()) { [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, 'Process') }
        }
    }
} finally { Pop-Location }
