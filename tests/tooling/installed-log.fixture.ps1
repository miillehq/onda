param([Parameter(Mandatory)][string]$Directory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '../../scripts/installed-log.ps1')

$file = Join-Path $Directory 'onda.log'
$newline = [Environment]::NewLine
function New-Marker([int]$ProcessId, [string]$Timestamp) {
    @{ timestamp = $Timestamp; level = 'INFO'; fields = @{ subsystem = 'application'; message = 'application started'; pid = $ProcessId; setup_ms = 91 } } | ConvertTo-Json -Compress
}
$stale = New-Marker 51 '2099-01-01T00:00:00Z'
$empty = Get-InstalledLogCheckpoint -Directory $Directory
[IO.File]::WriteAllText($file, $stale + $newline)
if (-not (Get-InstalledStartupMarker -Directory $Directory -Checkpoint $empty -ProcessId 51)) { throw 'A fresh-install log was missed.' }
$baseline = Get-InstalledLogCheckpoint -Directory $Directory
if (Get-InstalledStartupMarker -Directory $Directory -Checkpoint $baseline -ProcessId 51) { throw 'A stale future-dated entry was accepted.' }

$foreign = New-Marker 51 '2026-01-01T00:00:00Z'
$fresh = New-Marker 52 '2000-01-01T00:00:00Z'
$half = [int][Math]::Floor($fresh.Length / 2)
[IO.File]::AppendAllText($file, $foreign + $newline + $fresh.Substring(0, $half))
if (Get-InstalledStartupMarker -Directory $Directory -Checkpoint $baseline -ProcessId 52) { throw 'An incomplete or foreign marker was accepted.' }
[IO.File]::AppendAllText($file, $fresh.Substring($half) + $newline)
$marker = Get-InstalledStartupMarker -Directory $Directory -Checkpoint $baseline -ProcessId 52
if ($null -eq $marker -or $marker.processId -ne 52 -or $marker.setupMs -ne 91) { throw 'The owned completed marker was missed after an earlier UTC timestamp.' }

$beforeRotation = Get-InstalledLogCheckpoint -Directory $Directory
$legacy = @{ timestamp = '2025-01-01T00:00:00Z'; level = 'INFO'; fields = @{ subsystem = 'application'; message = 'application started' } } | ConvertTo-Json -Compress
[IO.File]::WriteAllText((Join-Path $Directory 'onda-next.log'), $legacy + $newline)
if (-not (Get-InstalledStartupMarker -Directory $Directory -Checkpoint $beforeRotation -ProcessId 53)) { throw 'A new log file from an older candidate was ignored.' }

$attempts = @{ count = 0 }
$candidateProfile = Join-Path $Directory 'candidate'
$warm = Join-Path $candidateProfile 'first-launch.bin'
$ready = Invoke-InstalledAppCheck -ProfileParent $Directory -ProfileName candidate -Attempt {
    $attempts.count++
    if ($attempts.count -eq 1) {
        [IO.Directory]::CreateDirectory($candidateProfile) | Out-Null
        [IO.File]::WriteAllText($warm, 'first attempt')
        throw (New-InstalledStartupTimeout -ProcessId 52)
    }
    if (Test-Path -LiteralPath $warm) { throw 'The retry reused the first launch profile.' }
    'ready'
}
if ($ready -ne 'ready' -or $attempts.count -ne 2) { throw 'A single startup retry did not run.' }
$archives = @(Get-ChildItem -LiteralPath $Directory -Directory -Filter 'candidate.first-launch-*')
if ($archives.Count -ne 1 -or [IO.File]::ReadAllText((Join-Path $archives[0].FullName 'first-launch.bin')) -ne 'first attempt') {
    throw 'The first launch profile was not preserved.'
}
$protected = Join-Path $Directory 'existing-profile'
[IO.Directory]::CreateDirectory($protected) | Out-Null
$attempts.count = 0
try { Invoke-InstalledAppCheck -ProfileParent $Directory -ProfileName existing-profile -Attempt { $attempts.count++ } } catch {}
if ($attempts.count -ne 0 -or -not (Test-Path -LiteralPath $protected)) { throw 'A pre-existing profile was used.' }

$attempts.count = 0
$terminal = $false
try { Invoke-InstalledAppCheck -ProfileParent $Directory -ProfileName interrupted -Attempt { $attempts.count++; throw 'Worker protocol mismatch.' } }
catch { $terminal = $_.Exception.Message -eq 'Worker protocol mismatch.' }
if (-not $terminal -or $attempts.count -ne 1) { throw 'An unrelated installer failure must propagate on its first attempt.' }
$attempts.count = 0
$terminal = $false
try { Invoke-InstalledAppCheck -ProfileParent $Directory -ProfileName repeated -Attempt { $attempts.count++; throw (New-InstalledStartupTimeout -ProcessId 52) } }
catch { $terminal = $_.FullyQualifiedErrorId.Split(',')[0] -eq 'Onda.InstalledStartupTimeout' }
if (-not $terminal -or $attempts.count -ne 2) { throw 'Two timeouts must fail the gate.' }

$shellProcess = [Diagnostics.Process]::GetCurrentProcess()
try {
    $bound = Get-InstalledLogCheckpoint -Directory $Directory
    foreach ($hasMarker in @($false, $true)) {
        if ($hasMarker) { [IO.File]::AppendAllText($file, (New-Marker $shellProcess.Id '2026-01-01T00:00:00Z') + $newline) }
        $terminal = $false
        try { $null = Wait-InstalledStartupMarker -Directory $Directory -Checkpoint $bound -ShellProcess $shellProcess -Clock ([Diagnostics.Stopwatch]::StartNew()) -Limit ([TimeSpan]::Zero) }
        catch { $terminal = $_.FullyQualifiedErrorId.Split(',')[0] -eq 'Onda.InstalledStartupTimeout' }
        if (-not $terminal) { throw "An expired observation was accepted (marker present: $hasMarker)." }
    }
    $accepted = Wait-InstalledStartupMarker -Directory $Directory -Checkpoint $bound -ShellProcess $shellProcess -Clock ([Diagnostics.Stopwatch]::StartNew()) -Limit ([TimeSpan]::FromSeconds(30))
    if ($accepted.processId -ne $shellProcess.Id) { throw 'The live in-budget marker was missed.' }
} finally { $shellProcess.Dispose() }
