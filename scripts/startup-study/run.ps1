$ErrorActionPreference = 'Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') { throw 'Requires a disposable Windows runner.' }
$folder = Join-Path (Get-Location) '.artifacts/startup-study'
$null = New-Item -ItemType Directory -Force -Path $folder
$env:ONDA_TRACE_OUTPUT = $folder
$env:ONDA_TRACE_RAW = Join-Path $env:RUNNER_TEMP 'onda-native-events.etl'
$env:ONDA_TRACE_PHASES = Join-Path $folder 'wry-stages.json'
$webviewRoot = Join-Path ${env:ProgramFiles(x86)} 'Microsoft/EdgeWebView/Application'
$versions = @(Get-ChildItem -LiteralPath $webviewRoot -Directory -ErrorAction SilentlyContinue | Where-Object Name -match '^\d+\.\d+\.\d+\.\d+$' | Select-Object -ExpandProperty Name)
@{ runnerImage=$env:ImageVersion; webviewRuntime=$versions; logicalProcessors=[Environment]::ProcessorCount } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $folder 'environment.json') -Encoding utf8
try {
    ./scripts/installer-smoke.ps1 -InstallerPath $args[0] | Select-Object status,startupSeconds,nativeSetupMs
} finally {
    if (Test-Path -LiteralPath $env:ONDA_TRACE_RAW) {
        $decoded = Join-Path $env:RUNNER_TEMP 'onda-native-events.xml'
        tracerpt.exe $env:ONDA_TRACE_RAW -o $decoded -of XML -y | Out-Null
        if ($LASTEXITCODE) { throw 'Event trace could not be decoded.' }
        python scripts/startup-study/events.py $decoded $folder
        if ($LASTEXITCODE) { throw 'Event summarization failed.' }
    }
    if (Test-Path -LiteralPath "$env:RUNNER_TEMP/onda-paging.etl") {
        tracerpt.exe "$env:RUNNER_TEMP/onda-paging.etl" -o "$env:RUNNER_TEMP/onda-paging.xml" -of XML -y | Out-Null
        if ($LASTEXITCODE) { throw 'Kernel trace decoding failed.' }
        python scripts/startup-study/paging.py "$env:RUNNER_TEMP/onda-paging.xml" $folder
        if ($LASTEXITCODE) { throw 'Kernel trace analysis failed.' }
    }
}
