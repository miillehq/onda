param(
    [ValidateSet('whisper-tiny-en', 'whisper-base-en-q5', 'whisper-small-en-q5')]
    [string] $ModelId = 'whisper-tiny-en',
    [string] $ModelDirectory = (Join-Path $env:LOCALAPPDATA 'Onda\models'),
    [ValidateRange(1, 20)]
    [int] $Repetitions = 3,
    [ValidateSet('cpu', 'auto', 'gpu')]
    [string] $ExecutionPreference = 'cpu',
    [switch] $CheckVulkan,
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repositoryDirectory
try {
    if (-not $SkipBuild) {
        cargo build -p onda-asr --release --features native-asr --bins
        if ($LASTEXITCODE -ne 0) { throw 'Native ASR benchmark build failed. See docs/asr.md for LLVM/CMake requirements.' }
    }
    $benchmarkExecutable = Join-Path $repositoryDirectory 'target\release\onda-asr-benchmark.exe'
    $workerDirectory = Join-Path $repositoryDirectory 'target\release'
    if (-not (Test-Path -LiteralPath $benchmarkExecutable -PathType Leaf)) { throw 'ASR benchmark executable is missing.' }
    $fixture = Join-Path $repositoryDirectory 'tests\fixtures\asr\jfk.wav'
    $expectedFixtureHash = '59DFB9A4ACB36FE2A2AFFC14BACBEE2920FF435CB13CC314A08C13F66BA7860E'
    if ((Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash -ne $expectedFixtureHash) { throw 'ASR fixture checksum does not match.' }
    $installationJson = & $benchmarkExecutable download $ModelDirectory $ModelId
    if ($LASTEXITCODE -ne 0) { throw 'Verified model installation failed.' }
    $installation = ($installationJson -join "`n") | ConvertFrom-Json
    $modelPath = $installation.path
    if (-not $modelPath -or -not (Test-Path -LiteralPath $modelPath -PathType Leaf)) { throw 'Model manager did not return an installed file.' }
    & $benchmarkExecutable hardware
    & $benchmarkExecutable $modelPath $fixture $workerDirectory $Repetitions $ExecutionPreference
    if ($LASTEXITCODE -ne 0) { throw 'Real-model transcription failed.' }
    $env:ONDA_ASR_TEST_MODEL = $modelPath
    $env:ONDA_ASR_WORKER_DIR = $workerDirectory
    cargo test -p onda-asr --test golden -- --ignored
    if ($LASTEXITCODE -ne 0) { throw 'Semantic transcription golden test failed.' }
    if ($CheckVulkan) {
        cargo test -p onda-asr --test gpu_golden -- --ignored
        if ($LASTEXITCODE -ne 0) { throw 'Physical Vulkan backend recognition acceptance failed.' }
    }
} finally {
    Pop-Location
}
