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
    $product = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    $machine = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate'
    $current = Get-ItemProperty -LiteralPath "$machine/Clients/$product" -ErrorAction SilentlyContinue
    $state = Get-ItemProperty -LiteralPath "$machine/ClientState/$product" -ErrorAction SilentlyContinue
    $expected = if ($current.pv) { Join-Path $webviewRoot $current.pv }
    $loader = if ($expected) { Get-Item -LiteralPath (Join-Path $expected 'EBWebView/x64/EmbeddedBrowserWebView.dll') -ErrorAction SilentlyContinue }
    @{ systemVersion = $current.pv; systemRegistrationMatches = ($expected -and $state.EBWebView -eq $expected);
       loaderBytes = $loader.Length; userClient = (Test-Path -LiteralPath "HKCU:/Software/Microsoft/EdgeUpdate/Clients/$product");
       overrideVariableNames = @(Get-ChildItem Env: | Where-Object Name -Like 'WEBVIEW2_*' | Select-Object -ExpandProperty Name) } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $folder 'registry.json') -Encoding utf8
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
