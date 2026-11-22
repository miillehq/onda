function Invoke-CacheCondition {
    param([string]$OutputDirectory)
    $sample = [int]$env:ONDA_STUDY_SAMPLE
    $arm = @('cold-idle','warm-idle','cold-busy','warm-busy')[$sample % 4]
    $folder = Join-Path $env:LOCALAPPDATA ('onda-storage-experiment-' + [guid]::NewGuid().ToString('N'))
    $null = New-Item -ItemType Directory -Path $folder
    $target = Join-Path $folder 'local-read-fixture.bin'
    # A private preallocated file is the only storage target used by DiskSpd.
    $block = [byte[]]::new(1MB)
    $random = [Random]::new(63)
    $stream = [IO.File]::Open($target,'CreateNew','Write','None')
    try { for ($i=0; $i -lt 192; $i++) { $random.NextBytes($block); $stream.Write($block,0,$block.Length) }; $stream.Flush($true) } finally { $stream.Dispose() }
    $runtime = Join-Path ${env:ProgramFiles(x86)} 'Microsoft/EdgeWebView/Application'
    $image = Get-ChildItem -LiteralPath $runtime -File -Recurse -Filter msedge.dll | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if (-not $image) {throw 'Microsoft runtime image unavailable.'}
    $root = $image.Directory.FullName
    $names = @('msedge.dll','msedgewebview2.exe','msedge_elf.dll','embeddedbrowserwebview.dll','libglesv2.dll','libegl.dll','vk_swiftshader.dll','vulkan-1.dll','icudtl.dat','d3dcompiler_47.dll','v8_context_snapshot.bin','resources.pak','oneds.dll','telclient.dll','ffmpeg.dll','en-us.pak')
    $files = @(Get-ChildItem -LiteralPath $root -File -Recurse | Where-Object {$_.Name -in $names})
    $before = @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue).Count
    $duration = [Diagnostics.Stopwatch]::StartNew(); $count=0L
    if ($arm.StartsWith('warm')) {
        foreach ($file in $files) {
            $stream = [IO.File]::OpenRead($file.FullName)
            try { while (($bytes=$stream.Read($block,0,$block.Length)) -gt 0) {$count+=$bytes} } finally {$stream.Dispose()}
        }
    }
    $workMs=$duration.ElapsedMilliseconds
    $remaining=12000-$duration.ElapsedMilliseconds
    if ($remaining -gt 0) {Start-Sleep -Milliseconds $remaining}
    if ($arm.EndsWith('busy')) {
        $tool = Get-ChildItem -LiteralPath "$env:RUNNER_TEMP/onda-diskspd" -Recurse -File -Filter diskspd.exe | Where-Object {$_.Directory.Name -match '^amd64(free)?$'} | Select-Object -First 1
        if (-not $tool) {throw 'No x64 DiskSpd executable in the verified archive.'}
        $inputArgs=@('-d55','-W0','-C0','-b64K','-r','-o32','-t2','-Su','-w0','-L','-Rxml',('"' + $target + '"'))
        $loadProcess=Start-Process -FilePath $tool.FullName -ArgumentList $inputArgs -RedirectStandardOutput "$env:RUNNER_TEMP/onda-diskspd.xml" -RedirectStandardError "$env:RUNNER_TEMP/onda-diskspd.err" -WindowStyle Hidden -PassThru
        Start-Sleep -Milliseconds 500
        if ($loadProcess.HasExited) {throw 'Local read workload exited early.'}
    }
    @{condition=$arm;readMs=$workMs;bytes=$count;imageFiles=$files.Count;pretreatmentMs=$duration.ElapsedMilliseconds;existingBrowserProcesses=$before;profileExisted=(Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'com.miille.onda'))} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'experiment.json') -Encoding utf8
}
