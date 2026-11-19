function Get-InstalledLogCheckpoint {
    <# Capture the end of each closed log before launching the owned application. #>
    param([Parameter(Mandatory)][string]$Directory)
    $checkpoint = @{}
    if (Test-Path -LiteralPath $Directory -PathType Container) {
        foreach ($file in Get-ChildItem -LiteralPath $Directory -File) {
            $checkpoint[$file.Name] = [pscustomobject]@{ Bytes = $file.Length; Created = $file.CreationTimeUtc.Ticks }
        }
    }
    return $checkpoint
}

function Read-InstalledLogEntries {
    <# Stream only bytes written after the checkpoint. A partial JSON line can finish on the next poll. #>
    param([Parameter(Mandatory)][string]$Directory, [Parameter(Mandatory)][hashtable]$Checkpoint)
    if (-not (Test-Path -LiteralPath $Directory -PathType Container)) { return }
    $share = [System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete
    foreach ($file in Get-ChildItem -LiteralPath $Directory -File) {
        try { $stream = [System.IO.File]::Open($file.FullName, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, $share) }
        catch [System.IO.FileNotFoundException] { continue }
        catch [System.IO.DirectoryNotFoundException] { continue }
        try {
            $previous = $Checkpoint[$file.Name]
            if ($null -ne $previous -and $previous.Created -eq $file.CreationTimeUtc.Ticks -and $stream.Length -ge $previous.Bytes) {
                if ($stream.Length -eq $previous.Bytes) { continue }
                $null = $stream.Seek($previous.Bytes, [System.IO.SeekOrigin]::Begin)
            }
            $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::UTF8, $true, 4096, $true)
            try {
                while ($null -ne ($line = $reader.ReadLine())) {
                    try {
                        $entry = $line | ConvertFrom-Json -ErrorAction Stop
                        $time = if ($entry.timestamp -is [DateTime]) { $entry.timestamp.ToUniversalTime() } else { [DateTimeOffset]::Parse([string]$entry.timestamp).UtcDateTime }
                        $fields = $entry.fields.PSObject.Properties
                        if (-not $fields['message']) { continue }
                        [pscustomobject]@{
                            timestampUtc = $time
                            level = [string]$entry.level
                            subsystem = if ($fields['subsystem']) { [string]$fields['subsystem'].Value } else { '' }
                            message = [string]$fields['message'].Value
                            processId = if ($fields['pid']) { [int]$fields['pid'].Value } else { $null }
                            setupMs = if ($fields['setup_ms']) { [long]$fields['setup_ms'].Value } else { $null }
                        }
                    } catch { continue }
                }
            } finally { $reader.Dispose() }
        } finally { $stream.Dispose() }
    }
}

function Get-InstalledStartupMarker {
    <# Current builds identify the owned shell. Older builds still have a fresh, single-instance log boundary. #>
    param([Parameter(Mandatory)][string]$Directory, [Parameter(Mandatory)][hashtable]$Checkpoint, [Parameter(Mandatory)][int]$ProcessId)
    Read-InstalledLogEntries -Directory $Directory -Checkpoint $Checkpoint |
        Where-Object { $_.subsystem -eq 'application' -and $_.message -eq 'application started' -and ($null -eq $_.processId -or $_.processId -eq $ProcessId) } |
        Select-Object -First 1
}

function Select-InstalledStartupPhases {
    param([AllowEmptyCollection()][array]$Entries, [int]$ProcessId)
    $Entries | Where-Object { $_.processId -eq $ProcessId -and $_.subsystem -in @('startup', 'application') } |
        Select-Object message, setupMs
}

function New-InstalledStartupTimeout {
    param([Parameter(Mandatory)][int]$ProcessId)
    [Management.Automation.ErrorRecord]::new(
        [TimeoutException]::new('Installed application startup marker was not observed.'),
        'Onda.InstalledStartupTimeout',
        [Management.Automation.ErrorCategory]::OperationTimeout,
        $ProcessId)
}

function Wait-InstalledStartupMarker {
    <# A marker is accepted only while the original launch clock is inside its observation budget. #>
    param(
        [Parameter(Mandatory)][string]$Directory,
        [Parameter(Mandatory)][hashtable]$Checkpoint,
        [Parameter(Mandatory)][Diagnostics.Process]$ShellProcess,
        [Parameter(Mandatory)][Diagnostics.Stopwatch]$Clock,
        [Parameter(Mandatory)][TimeSpan]$Limit
    )
    while (-not $ShellProcess.HasExited) {
        $marker = Get-InstalledStartupMarker -Directory $Directory -Checkpoint $Checkpoint -ProcessId $ShellProcess.Id
        if ($Clock.Elapsed -ge $Limit) { throw (New-InstalledStartupTimeout -ProcessId $ShellProcess.Id) }
        if ($marker) { return $marker }
        Start-Sleep -Milliseconds 250
    }
    throw 'Installed shell exited before it finished starting.'
}

function Invoke-InstalledAppCheck {
    <# L7 permits one fresh install after this specific readiness timeout. Each attempt writes its own report. #>
    param([Parameter(Mandatory)][scriptblock]$Attempt)
    try { & $Attempt; return }
    catch {
        if ($_.FullyQualifiedErrorId.Split(',')[0] -ne 'Onda.InstalledStartupTimeout') { throw }
        Write-Warning 'The installed application missed its startup deadline. The first report is retained. Retrying once (Onda issue #63).'
    }
    & $Attempt
}
