param([switch]$SkipBuild, [switch]$UnavailableCloudStorage, [switch]$WithDictationFixture, [string]$FixtureModelDirectory=(Join-Path $env:LOCALAPPDATA 'Onda/models'))
$ErrorActionPreference = 'Stop'
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repositoryDirectory
try {
    if (-not $SkipBuild) {
        & (Join-Path $PSScriptRoot 'prepare.ps1')
        npm run contracts
        if ($LASTEXITCODE -ne 0) { throw 'IPC contract generation failed.' }
        npm run build
        if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
        cargo build --locked -p onda-app --bin onda
        if ($LASTEXITCODE -ne 0) { throw 'Debug desktop build failed.' }
    }
    $smokeDirectory = Join-Path $repositoryDirectory ('.local/smoke-' + [guid]::NewGuid().ToString('N'))
    [System.IO.Directory]::CreateDirectory($smokeDirectory) | Out-Null
    if ($UnavailableCloudStorage) { [System.IO.File]::WriteAllText((Join-Path $smokeDirectory 'accounts'), 'immutable-storage-sentinel') }
    $previousSmokeDirectory = $env:ONDA_SMOKE_DIRECTORY
    $previousReplay = $env:ONDA_SMOKE_REPLAY
    if ($WithDictationFixture) {
        $fixtureModelId = 'whisper-tiny-en'
        $fixtureModelRevision = '5359861c739e955e79d9a303bcbc70fb988958b1'
        $fixtureModelDigest = '921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f'
        $fixtureModelName = "$fixtureModelId-$fixtureModelRevision.bin"
        $fixtureSource = Join-Path $FixtureModelDirectory $fixtureModelName
        if (-not (Test-Path -LiteralPath $fixtureSource)) { throw 'Run scripts/asr-benchmark.ps1 to install the trusted fixture model, or specify FixtureModelDirectory.' }
        if ((Get-FileHash -LiteralPath $fixtureSource -Algorithm SHA256).Hash.ToLowerInvariant() -ne $fixtureModelDigest) { throw 'Fixture model checksum is invalid.' }
        $fixtureDestinationDirectory=Join-Path $smokeDirectory 'models'
        [System.IO.Directory]::CreateDirectory($fixtureDestinationDirectory)|Out-Null
        Copy-Item -LiteralPath $fixtureSource -Destination (Join-Path $fixtureDestinationDirectory $fixtureModelName)
    }
    $env:ONDA_SMOKE_DIRECTORY = $smokeDirectory
    $viteProcess = $null
    try {
        $env:ONDA_SMOKE_REPLAY = if ($WithDictationFixture) { '1' } else { $null }
        $nodeExecutable = (Get-Command node -ErrorAction Stop).Source
        $viteOutput = Join-Path $smokeDirectory 'vite.log'
        $viteProcess = Start-Process -FilePath $nodeExecutable -ArgumentList @('node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', '1420', '--strictPort') -WorkingDirectory $repositoryDirectory -WindowStyle Hidden -RedirectStandardOutput $viteOutput -RedirectStandardError (Join-Path $smokeDirectory 'vite-errors.log') -PassThru
        $serverClock = [System.Diagnostics.Stopwatch]::StartNew()
        $serverAnswered = $false
        do { try { $null = Invoke-WebRequest -Uri 'http://127.0.0.1:1420' -UseBasicParsing -TimeoutSec 1; $serverAnswered = $true; break } catch { Start-Sleep -Milliseconds 100 } } while ($serverClock.Elapsed.TotalSeconds -lt 5)
        $serverSeconds = [math]::Round($serverClock.Elapsed.TotalSeconds, 1)
        $process = Start-Process -FilePath (Join-Path $repositoryDirectory 'target/debug/onda.exe') -WorkingDirectory $repositoryDirectory -WindowStyle Hidden -PassThru
        $smokeTimeout = if($WithDictationFixture){180000}else{30000}
        if (-not $process.WaitForExit($smokeTimeout)) {
            $process.Kill()
            # Say where the time went: the dev server, Vite's own output and the app's log messages.
            $diagnosis = @("The dev server answered: $serverAnswered (checked for $serverSeconds s).")
            $logDirectory = Join-Path $smokeDirectory 'logs'
            if (Test-Path -LiteralPath $logDirectory) {
                $diagnosis += @(Get-ChildItem -LiteralPath $logDirectory -File | ForEach-Object { Get-Content -LiteralPath $_.FullName } | ForEach-Object {
                    try { $entry = $_ | ConvertFrom-Json; "app: $($entry.timestamp) $($entry.level) $($entry.fields.subsystem) $($entry.fields.message)" } catch { }
                } | Select-Object -Last 30)
            }
            foreach ($file in @($viteOutput, (Join-Path $smokeDirectory 'vite-errors.log'))) {
                if (Test-Path -LiteralPath $file) { $diagnosis += @(Get-Content -LiteralPath $file -Tail 20 | ForEach-Object { "vite: $_" }) }
            }
            throw ("Hidden desktop IPC smoke timed out.`n" + ($diagnosis -join "`n"))
        }
        $reportPath = Join-Path $smokeDirectory 'report.json'
        if (-not (Test-Path -LiteralPath $reportPath)) { throw 'The native desktop did not produce a smoke report.' }
        $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
        if (-not $report.passed -or $process.ExitCode -ne 0) { throw ('Desktop IPC smoke failed after: ' + ($report.checks -join ', ')) }
        $report | ConvertTo-Json -Depth 3
        if ($UnavailableCloudStorage -and [System.IO.File]::ReadAllText((Join-Path $smokeDirectory 'accounts')) -ne 'immutable-storage-sentinel') { throw 'Protected storage sentinel was unexpectedly changed.' }
    } finally { if ($viteProcess -and -not $viteProcess.HasExited) { $viteProcess.Kill() }; $env:ONDA_SMOKE_DIRECTORY = $previousSmokeDirectory; $env:ONDA_SMOKE_REPLAY=$previousReplay }
} finally { Pop-Location }
