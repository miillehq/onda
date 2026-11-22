function Invoke-CacheCondition {
    param([string]$OutputDirectory)
    $sample = [int]$env:ONDA_STUDY_SAMPLE
    $arm = @('immediate','wait','read-image')[$sample % 3]
    $profilePath = Join-Path $env:LOCALAPPDATA 'com.miille.onda'
    $before = @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue).Count
    $duration = [Diagnostics.Stopwatch]::StartNew()
    $count = 0L
    if ($arm -eq 'read-image') {
        $runtime = Join-Path ${env:ProgramFiles(x86)} 'Microsoft/EdgeWebView/Application'
        $binary = Get-ChildItem -LiteralPath $runtime -Filter msedge.dll -File -Recurse | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
        if (-not $binary) {throw 'Missing Edge WebView runtime image.'}
        $stream = [IO.File]::OpenRead($binary.FullName)
        try {
            $buffer = [byte[]]::new(1048576)
            while (($bytes = $stream.Read($buffer,0,$buffer.Length)) -gt 0) { $count += $bytes }
        } finally { $stream.Dispose() }
    }
    $workMs = $duration.ElapsedMilliseconds
    if ($arm -ne 'immediate') {
        $remaining = 10000 - $duration.ElapsedMilliseconds
        if ($remaining -gt 0) {Start-Sleep -Milliseconds $remaining}
    }
    @{condition=$arm;readMs=$workMs;bytes=$count;pretreatmentMs=$duration.ElapsedMilliseconds;existingBrowserProcesses=$before;profileExisted=(Test-Path -LiteralPath $profilePath)} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'experiment.json') -Encoding utf8
}
