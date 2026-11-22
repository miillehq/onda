function Invoke-CacheCondition {
    param([string]$OutputDirectory)
    $sample = [int]$env:ONDA_STUDY_SAMPLE
    $arm = if ($sample % 4 -eq 0) {'idle'} else {'busy'}
    $folder = Join-Path $env:LOCALAPPDATA ('onda-storage-experiment-' + [guid]::NewGuid().ToString('N'))
    $null = New-Item -ItemType Directory -Path $folder
    $target = Join-Path $folder 'local-read-fixture.bin'
    # A private preallocated file is the only storage target used by DiskSpd.
    $block = [byte[]]::new(1MB)
    $random = [Random]::new(63)
    $stream = [IO.File]::Open($target,'CreateNew','Write','None')
    try { for ($i=0; $i -lt 192; $i++) { $random.NextBytes($block); $stream.Write($block,0,$block.Length) }; $stream.Flush($true) } finally { $stream.Dispose() }
    $before = @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue).Count
    $duration = [Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds 12
    if ($arm -eq 'busy') {
        $tool = Get-ChildItem -LiteralPath "$env:RUNNER_TEMP/onda-diskspd" -Recurse -File -Filter diskspd.exe | Where-Object {$_.Directory.Name -match '^amd64(free)?$'} | Select-Object -First 1
        if (-not $tool) {throw 'No x64 DiskSpd executable in the verified archive.'}
        $inputArgs=@('-d55','-W0','-C0','-b64K','-r','-o32','-t2','-Su','-w0','-L','-Rxml',('"' + $target + '"'))
        $loadProcess=Start-Process -FilePath $tool.FullName -ArgumentList $inputArgs -RedirectStandardOutput "$env:RUNNER_TEMP/onda-diskspd.xml" -RedirectStandardError "$env:RUNNER_TEMP/onda-diskspd.err" -WindowStyle Hidden -PassThru
        $env:ONDA_STUDY_LOAD_PID = $loadProcess.Id
        Start-Sleep -Milliseconds 500
        if ($loadProcess.HasExited) {throw 'Local read workload exited early.'}
    }
    @{condition=$arm;variant=$env:ONDA_STUDY_VARIANT;position=$env:ONDA_STUDY_POSITION;readMs=0;bytes=0;pretreatmentMs=$duration.ElapsedMilliseconds;existingBrowserProcesses=$before;profileExisted=(Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'com.miille.onda'))} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'experiment.json') -Encoding utf8
}
