# Windows on ARM: builds the CPU speech worker inside the ARM64 developer environment. prepare.ps1
# starts this script as a process of its own, so none of the many variables that environment
# changes reaches the Rust builds that follow in the caller's shell. Putting them back by hand was
# not enough on the runners: a leftover VCINSTALLDIR made rustc link with Git's `link`, and after
# a full restore the next build still linked without the Windows SDK libraries (kernel32.lib).
param([Parameter(Mandatory = $true)][ValidateSet('debug', 'release')][string]$BuildMode)
$ErrorActionPreference = 'Stop'
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repositoryDirectory
. (Join-Path $PSScriptRoot 'arm64-toolchain.ps1')
# A whisper.cpp CMake cache made with another compiler cannot be re-configured for clang-cl: CMake
# resets it mid-run and fails. Such a build directory (build output only) starts afresh.
$compilerLine = '^CMAKE_C_COMPILER:[A-Z]+=' + [regex]::Escape($env:CMAKE_C_COMPILER) + '$'
foreach ($cache in @(Get-Item -Path (Join-Path $repositoryDirectory "target/$BuildMode/build/whisper-rs-sys-*/out/build/CMakeCache.txt") -ErrorAction SilentlyContinue)) {
    if (-not (Select-String -LiteralPath $cache.FullName -Pattern $compilerLine -Quiet)) {
        Write-Host "Removing a whisper.cpp CMake build made with another compiler: $($cache.Directory.FullName)"
        Remove-Item -LiteralPath $cache.Directory.FullName -Recurse -Force
    }
}
[string[]]$releaseArguments = @()
if ($BuildMode -eq 'release') { $releaseArguments = @('--release') }
cargo build --locked -p onda-asr --features native-asr --bin onda-asr-worker @releaseArguments
exit $LASTEXITCODE
