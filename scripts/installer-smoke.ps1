<#
.SYNOPSIS
Install, quietly launch, inspect and uninstall a candidate in a fresh owned artifact directory.
.DESCRIPTION
Refuses existing Onda installer registrations/processes. Never deletes application data.
Stops only the shell/descendants created by this test, preserving helper EOF exit evidence.
Returns one report object and saves the report under .artifacts/installed-smoke.
#>
[CmdletBinding()]
param(
    [string]$InstallerPath = '',
    [switch]$WithVulkan,
    [ValidateRange(3, 10)][int]$ObservationSeconds = 5
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'installed-notices.ps1')
. (Join-Path $PSScriptRoot 'installed-log.ps1')
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
$configuration = Get-Content -LiteralPath (Join-Path $repositoryDirectory 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$productName = [string]$configuration.productName
$applicationId = [string]$configuration.identifier
# The installed workers must answer with the protocol this checkout speaks; read it, never copy it.
$protocolSource = Get-Content -LiteralPath (Join-Path $repositoryDirectory 'crates/onda-asr/src/protocol.rs') -Raw
if ($protocolSource -notmatch 'pub const PROTOCOL_VERSION: u32 = (\d+);') { throw 'Worker protocol version not found in protocol.rs.' }
$workerProtocolVersion = [int]$Matches[1]
$artifactRoot = [System.IO.Path]::GetFullPath((Join-Path $repositoryDirectory '.artifacts/installed-smoke'))
$testId = [guid]::NewGuid().ToString('N')
$installDirectory = Join-Path $artifactRoot $testId
$reportPath = Join-Path $artifactRoot "$testId.json"
$applicationDataDirectory = Join-Path $env:LOCALAPPDATA $applicationId
$logDirectory = Join-Path $applicationDataDirectory 'logs'
$startupLimit = [TimeSpan]::FromSeconds(30)
$helperStartupLimit = [TimeSpan]::FromSeconds(15)
$uninstallExitLimit = [TimeSpan]::FromSeconds(10)
$startupClock = $null
$logCheckpoint = $null
$ownedProcesses = [System.Collections.Generic.List[System.Diagnostics.Process]]::new()
$shell = $null
$helper = $null
$installed = $false
$installerAttempted = $false
# NSIS remembers the last install folder under HKCU\Software\<publisher>\<product> and offers it next time.
$rememberedFolderKey = "HKCU:\Software\$($applicationId.Split('.')[1])\$productName"
$report = [ordered]@{ formatVersion = 1; testId = $testId; hostOnly = $true; status = 'running' }

function Get-RememberedFolder {
    $key = Get-Item -LiteralPath $rememberedFolderKey -ErrorAction SilentlyContinue
    if ($key) { return $key.GetValue('') }
    return $null
}

function Assert-OwnedInstallPath {
    param([string]$Path)
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $allowedPrefix = $artifactRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($allowedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Installer test path is outside its owned artifact directory.' }
    return $fullPath
}

function Get-DataHashes {
    # Time O(total file bytes), space O(file count). Contents never enter the report.
    param([string]$Directory)
    $hashes = @{}
    if (-not (Test-Path -LiteralPath $Directory -PathType Container)) { return $hashes }
    foreach ($file in Get-ChildItem -LiteralPath $Directory -File -Recurse) {
        $relative = $file.FullName.Substring($Directory.TrimEnd([System.IO.Path]::DirectorySeparatorChar).Length + 1)
        $hashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    return $hashes
}

function Start-OwnedProcess {
    param([string]$Path, [string]$Arguments = '', [switch]$RedirectOutput)
    $info = [System.Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Path
    $info.Arguments = $Arguments
    $info.WorkingDirectory = $installDirectory
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $info.RedirectStandardOutput = [bool]$RedirectOutput
    $info.RedirectStandardError = [bool]$RedirectOutput
    $info.EnvironmentVariables['PATH'] = (Join-Path $env:SystemRoot 'System32') + ';' + $env:SystemRoot
    $info.EnvironmentVariables.Remove('ONDA_SMOKE_DIRECTORY')
    $process = [System.Diagnostics.Process]::Start($info)
    $null = $process.Handle
    $ownedProcesses.Add($process)
    return $process
}

function Get-OwnedDescendants {
    # Time O(p * depth), space O(p), for the small process tree owned by this test.
    param([System.Diagnostics.Process]$Parent)
    $knownIds = [System.Collections.Generic.HashSet[uint32]]::new()
    $null = $knownIds.Add([uint32]$Parent.Id)
    $children = [System.Collections.Generic.List[System.Diagnostics.Process]]::new()
    $processes = @(Get-CimInstance Win32_Process)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($candidate in $processes) {
            if ($knownIds.Contains([uint32]$candidate.ProcessId) -or -not $knownIds.Contains([uint32]$candidate.ParentProcessId)) { continue }
            try {
                $process = [System.Diagnostics.Process]::GetProcessById([int]$candidate.ProcessId)
                if ($process.StartTime.ToUniversalTime() -lt $Parent.StartTime.ToUniversalTime()) { continue }
                $null = $process.Handle
                $null = $knownIds.Add([uint32]$candidate.ProcessId)
                $children.Add($process)
                $changed = $true
            } catch [System.ArgumentException] { continue }
        }
    }
    return $children.ToArray()
}

function Remember-OwnedDescendants {
    param([System.Diagnostics.Process]$Parent)
    if ($Parent.HasExited) { return }
    $children = @(Get-OwnedDescendants -Parent $Parent)
    if ($Parent.HasExited) { return }
    foreach ($child in $children) {
        if (-not ($ownedProcesses | Where-Object Id -EQ $child.Id)) { $ownedProcesses.Add($child) }
    }
}

function Test-InstalledWorker {
    param([string]$Name, [switch]$Optional)
    $process = Start-OwnedProcess -Path (Join-Path $installDirectory $Name) -Arguments '--probe' -RedirectOutput
    $output = $process.StandardOutput.ReadToEndAsync()
    $errors = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(10000)) { $process.Kill(); throw 'Installed worker probe timed out.' }
    if ($process.ExitCode -ne 0) {
        if ($Optional) { return [ordered]@{ binary = $Name; available = $false; exitCode = $process.ExitCode } }
        throw 'Installed CPU worker could not load or probe.'
    }
    $json = $output.Result
    if ($json.Length -gt 8192) { throw 'Installed worker probe output exceeded its bound.' }
    $probe = $json | ConvertFrom-Json
    if ($probe.version -ne $workerProtocolVersion) { throw "Installed worker probe protocol mismatch: $($probe.version), expected $workerProtocolVersion." }
    return [ordered]@{ binary = $Name; available = $true; vulkanDevices = $probe.vulkan_devices; exitCode = $process.ExitCode }
}

function Wait-OwnedHelper {
    param([System.Diagnostics.Process]$Parent)
    $wait = [Diagnostics.Stopwatch]::StartNew()
    while ($wait.Elapsed -lt $helperStartupLimit) {
        if ($Parent.HasExited) { throw 'Installed shell exited during startup.' }
        $candidate = Get-CimInstance Win32_Process -Filter "ParentProcessId = $($Parent.Id)" | Where-Object { $_.Name -eq 'onda-input.exe' -and $_.ExecutablePath -eq (Join-Path $installDirectory 'onda-input.exe') } | Select-Object -First 1
        if ($candidate) {
            $process = [System.Diagnostics.Process]::GetProcessById([int]$candidate.ProcessId)
            $null = $process.Handle
            $ownedProcesses.Add($process)
            return $process
        }
        Start-Sleep -Milliseconds 100
    }
    throw 'Installed shell did not launch its owned shortcut helper.'
}

function Stop-OwnedShellAndChildren {
    param([System.Diagnostics.Process]$Parent, [System.Diagnostics.Process]$ShortcutHelper)
    $children = @(Get-OwnedDescendants -Parent $Parent)
    foreach ($child in $children) { if (-not ($ownedProcesses | Where-Object Id -EQ $child.Id)) { $ownedProcesses.Add($child) } }
    if (-not $Parent.HasExited) { $Parent.Kill(); $null = $Parent.WaitForExit(5000) }
    $helperExited = $ShortcutHelper.WaitForExit(5000)
    foreach ($child in $children) {
        if ($child.Id -eq $ShortcutHelper.Id -or $child.HasExited) { continue }
        $child.Kill(); $null = $child.WaitForExit(5000)
    }
    return $helperExited
}

function Invoke-OwnedUninstaller {
    if (-not $installed -and -not $installerAttempted) { return 0 }
    $null = Assert-OwnedInstallPath -Path $installDirectory
    $uninstaller = Join-Path $installDirectory 'uninstall.exe'
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) { throw 'Owned installation uninstaller is missing.' }
    $process = Start-Process -FilePath $uninstaller -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'Owned installer uninstallation failed.' }
    $wait = [Diagnostics.Stopwatch]::StartNew()
    while ((Test-Path -LiteralPath (Join-Path $installDirectory 'onda.exe')) -and $wait.Elapsed -lt $uninstallExitLimit) { Start-Sleep -Milliseconds 100 }
    if (Test-Path -LiteralPath (Join-Path $installDirectory 'onda.exe')) { throw 'Owned installation executable survived uninstallation.' }
    return $process.ExitCode
}

foreach ($registryRoot in @('HKCU:', 'HKLM:')) {
    foreach ($key in @("$registryRoot\Software\Microsoft\Windows\CurrentVersion\Uninstall\$productName", "$registryRoot\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\$productName")) {
        if (Test-Path -LiteralPath $key) { throw 'An existing Onda installation is registered; isolated installer smoke refuses to overwrite it.' }
    }
}
if (Get-CimInstance Win32_Process | Where-Object Name -EQ 'onda.exe') { throw 'An existing Onda shell is running; installer smoke refuses to interrupt it.' }
if (Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name $productName -ErrorAction SilentlyContinue) { throw 'An existing Onda autostart value occupies the uninstall namespace; smoke refuses to remove it.' }
foreach ($shortcut in @((Join-Path ([System.Environment]::GetFolderPath('DesktopDirectory')) "$productName.lnk"), (Join-Path ([System.Environment]::GetFolderPath('Programs')) "$productName.lnk"), (Join-Path ([System.Environment]::GetFolderPath('Programs')) "$productName/$productName.lnk"))) {
    if (Test-Path -LiteralPath $shortcut) { throw 'An existing Onda shortcut occupies the installer namespace; isolated smoke refuses to overwrite it.' }
}
if (-not $InstallerPath) {
    $candidate = Get-ChildItem -LiteralPath (Join-Path $repositoryDirectory 'target/release/bundle/nsis') -Filter '*setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $candidate) { throw 'Build a real NSIS release candidate first.' }
    $InstallerPath = $candidate.FullName
}
$InstallerPath = (Resolve-Path -LiteralPath $InstallerPath).Path
$null = Assert-OwnedInstallPath -Path $installDirectory
if (Test-Path -LiteralPath $installDirectory) { throw 'Fresh installer test directory unexpectedly exists.' }
[System.IO.Directory]::CreateDirectory($artifactRoot) | Out-Null
$beforeLaunchHashes = Get-DataHashes -Directory $applicationDataDirectory
$previousRememberedFolder = Get-RememberedFolder
$report.installerSha256 = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$report.installerBytes = (Get-Item -LiteralPath $InstallerPath).Length
$report.installDirectory = $installDirectory
$report.existingApplicationDataFiles = $beforeLaunchHashes.Count

try {
    $installerAttempted = $true
    $installation = Start-Process -FilePath $InstallerPath -ArgumentList @('/S', "/D=$installDirectory") -WindowStyle Hidden -Wait -PassThru
    if ($installation.ExitCode -ne 0) { throw 'Silent candidate installation failed.' }
    $installed = $true
    $binaryNames = @('onda.exe', 'onda-input.exe', 'onda-asr-worker.exe', 'onda-asr-avx2-worker.exe')
    if ($WithVulkan) { $binaryNames += 'onda-asr-vulkan-worker.exe' }
    foreach ($name in $binaryNames) { if (-not (Test-Path -LiteralPath (Join-Path $installDirectory $name))) { throw "Installed candidate executable missing: $name" } }
    $manifest = Get-Content -LiteralPath (Join-Path $installDirectory 'licenses/Microsoft-runtime-manifest.json') -Raw | ConvertFrom-Json
    Assert-InstalledNotices -InstallDirectory $installDirectory -Manifest $manifest
    foreach ($runtimeFile in $manifest.files) {
        $path = Join-Path $installDirectory $runtimeFile.name
        if (-not (Test-Path -LiteralPath $path)) { throw "Installed CRT DLL missing: $($runtimeFile.name)" }
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $runtimeFile.sha256) { throw 'Installed CRT DLL differs from its verified source hash.' }
    }
    $report.runtimeDllCount = $manifest.files.Count
    $report.mplSourceSha256 = (Get-FileHash -LiteralPath (Join-Path $installDirectory 'licenses/MPL-source.zip') -Algorithm SHA256).Hash.ToLowerInvariant()
    $installedExecutables = @($binaryNames | ForEach-Object { Join-Path $installDirectory $_ })
    $runtimeAudit = & (Join-Path $PSScriptRoot 'collect-msvc-runtime.ps1') -ExecutablePaths $installedExecutables -WithVulkan:$WithVulkan
    $report.auditedExecutables = $runtimeAudit.AuditedExecutables
    $report.cpuWorker = Test-InstalledWorker -Name 'onda-asr-worker.exe'
    # Optional: a CPU without AVX2 cannot run it, and Onda then uses the baseline worker.
    $report.avx2Worker = Test-InstalledWorker -Name 'onda-asr-avx2-worker.exe' -Optional
    if ($WithVulkan) { $report.vulkanWorker = Test-InstalledWorker -Name 'onda-asr-vulkan-worker.exe' -Optional }
    $logCheckpoint = Get-InstalledLogCheckpoint -Directory $logDirectory
    $report.launchUtc = [DateTime]::UtcNow.ToString('o')
    if ($env:ONDA_TRACE_RAW) {
        . (Join-Path $PSScriptRoot 'startup-study/cache.ps1')
        Invoke-CacheCondition $env:ONDA_TRACE_OUTPUT
        wpr.exe -start "$PSScriptRoot/startup-study/paging.wprp!OndaPaging.Light" -filemode -instancename OndaPaging
        if ($LASTEXITCODE) { throw 'Paging capture did not start.' }
        $providers = Join-Path $env:RUNNER_TEMP 'onda-trace-providers.txt'
        @('{E16EC3D2-BB0F-4E8F-BDB8-DE0BEA82DC3D} 0x3F0000054404 5',
          '{3A5F2396-5C8F-4F1F-9B67-6CCA6C990E61} 0x10000000202F 5',
          'Microsoft-Windows-Kernel-Process 0x10 5','Microsoft-Windows-CAPI2 0xffffffffffffffff 5') |
          Set-Content -Encoding ascii -LiteralPath $providers
        logman.exe start OndaStudy -ets -o $env:ONDA_TRACE_RAW -bs 128 -nb 16 128 -pf $providers
        if ($LASTEXITCODE) {throw 'Failed to start event observation.'}
    }
    $report.launchUtc = [DateTime]::UtcNow.ToString('o')
    $startupClock = [Diagnostics.Stopwatch]::StartNew()
    $shell = Start-OwnedProcess -Path (Join-Path $installDirectory 'onda.exe') -Arguments '--background'
    Remember-OwnedDescendants -Parent $shell
    $helper = Wait-OwnedHelper -Parent $shell
    $report.helperObservedSeconds = $startupClock.Elapsed.TotalSeconds
    Remember-OwnedDescendants -Parent $shell
    # Startup ends with the "application started" log entry, after the pill's WebView2 exists. A
    # first start on a fresh profile creates the WebView2 data folder; the time it took is recorded.
    $startupEntry = Wait-InstalledStartupMarker -Directory $logDirectory -Checkpoint $logCheckpoint -ShellProcess $shell -Clock $startupClock -Limit $startupLimit
    $startupClock.Stop()
    $report.startupSeconds = $startupClock.Elapsed.TotalSeconds
    $report.nativeSetupMs = $startupEntry.setupMs
    $entries = @(Read-InstalledLogEntries -Directory $logDirectory -Checkpoint $logCheckpoint)
    $report.startupPhases = @(Select-InstalledStartupPhases -Entries $entries -ProcessId $shell.Id)
    $report.startupLogObserved = $true
    Remember-OwnedDescendants -Parent $shell
    Start-Sleep -Milliseconds 1000
    $shell.Refresh(); $helper.Refresh()
    $shellCpuBefore = $shell.TotalProcessorTime.TotalSeconds
    $helperCpuBefore = $helper.TotalProcessorTime.TotalSeconds
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds $ObservationSeconds
    if ($shell.HasExited -or $helper.HasExited) { throw 'Installed shell or shortcut helper exited while idle.' }
    $shell.Refresh(); $helper.Refresh()
    $elapsed = $clock.Elapsed.TotalSeconds
    $report.observationSeconds = $elapsed
    $report.shell = [ordered]@{ pid = $shell.Id; privateMiB = $shell.PrivateMemorySize64 / 1MB; workingSetMiB = $shell.WorkingSet64 / 1MB; cpuCorePercent = 100 * ($shell.TotalProcessorTime.TotalSeconds - $shellCpuBefore) / $elapsed }
    $report.helper = [ordered]@{ pid = $helper.Id; privateMiB = $helper.PrivateMemorySize64 / 1MB; workingSetMiB = $helper.WorkingSet64 / 1MB; cpuCorePercent = 100 * ($helper.TotalProcessorTime.TotalSeconds - $helperCpuBefore) / $elapsed }
    $report.helperExitedOnParentClose = Stop-OwnedShellAndChildren -Parent $shell -ShortcutHelper $helper
    if (-not $report.helperExitedOnParentClose) { throw 'Owned helper did not exit after the shell closed its inherited pipes.' }
    $beforeUninstallHashes = Get-DataHashes -Directory $applicationDataDirectory
    $report.applicationDataBeforeUninstall = $beforeUninstallHashes
    $report.uninstallerExitCode = Invoke-OwnedUninstaller
    $installed = $false
    $installerAttempted = $false
    $afterUninstallHashes = Get-DataHashes -Directory $applicationDataDirectory
    foreach ($relative in $beforeUninstallHashes.Keys) {
        if (-not $afterUninstallHashes.ContainsKey($relative) -or $afterUninstallHashes[$relative] -ne $beforeUninstallHashes[$relative]) { throw 'Uninstallation changed or removed application data.' }
    }
    $report.applicationDataPreserved = $true
    $report.applicationDataAfterUninstall = $afterUninstallHashes
    $report.applicationDataBeforeLaunch = $beforeLaunchHashes
    $report.status = 'passed'
} catch {
    $report.status = 'failed'
    if ($env:ONDA_TRACE_RAW -and $_.FullyQualifiedErrorId.Split(',')[0] -eq 'Onda.InstalledStartupTimeout') {
        $report.deadlineObservedSeconds = $startupClock.Elapsed.TotalSeconds
        $lateMarker = $null
        $end = $startupClock.Elapsed.TotalSeconds + 60
        while (-not $shell.HasExited -and $startupClock.Elapsed.TotalSeconds -lt $end) {
            $lateMarker = Get-InstalledStartupMarker -Directory $logDirectory -Checkpoint $logCheckpoint -ProcessId $shell.Id
            if ($lateMarker) { break }
            Start-Sleep -Milliseconds 250
        }
        $report.lateReadyMs = if ($lateMarker) { $lateMarker.setupMs } else { $null }
    }
    $report.failure = $_.Exception.Message
    $report.failureId = $_.FullyQualifiedErrorId
    # What the installed app logged since launch (levels, subsystems and messages; never text or
    # audio, which Onda does not log) and which processes it started, to diagnose the failure.
    if ($null -ne $startupClock) {
        $report.observedSeconds = $startupClock.Elapsed.TotalSeconds
        if ($shell) { try { Remember-OwnedDescendants -Parent $shell } catch { $report.descendantInspectionFailure = $_.Exception.Message } }
        try {
            $entries = @(Read-InstalledLogEntries -Directory $logDirectory -Checkpoint $logCheckpoint)
            $report.logSinceLaunch = @($entries | ForEach-Object { "$($_.timestampUtc.ToString('o')) $($_.level) $($_.subsystem): $($_.message)" })
            if ($shell) { $report.startupPhases = @(Select-InstalledStartupPhases -Entries $entries -ProcessId $shell.Id) }
        } catch { $report.logInspectionFailure = $_.Exception.Message }
        try { $report.processesStarted = @($ownedProcesses | ForEach-Object { try { if (-not $_.HasExited) { $_.ProcessName } } catch [InvalidOperationException] { } } | Sort-Object -Unique) } catch { }
    }
    throw
} finally {
    if ($env:ONDA_TRACE_RAW -and $startupClock) { logman.exe stop OndaStudy -ets | Out-Null }
    if ($env:ONDA_TRACE_RAW -and $startupClock) {
        wpr.exe -stop "$env:RUNNER_TEMP/onda-paging.etl" -skipPdbGen -instancename OndaPaging | Out-Null
    }
    if ($shell) { try { Remember-OwnedDescendants -Parent $shell } catch { $report.descendantInspectionFailure = $_.Exception.Message } }
    foreach ($process in $ownedProcesses) {
        try { if (-not $process.HasExited) { $process.Kill(); $null = $process.WaitForExit(5000) } } catch [System.InvalidOperationException] {}
    }
    if ($installed -or ($installerAttempted -and (Test-Path -LiteralPath (Join-Path $installDirectory 'uninstall.exe')))) { try { $null = Invoke-OwnedUninstaller } catch { $report.cleanupFailure = $_.Exception.Message } }
    # Uninstall keeps the remembered folder; the next real installer must not offer this test's folder.
    try {
        if ((Get-RememberedFolder) -eq $installDirectory) {
            if ($previousRememberedFolder) { Set-Item -LiteralPath $rememberedFolderKey -Value $previousRememberedFolder }
            else { Remove-Item -LiteralPath $rememberedFolderKey -Recurse }
        }
    } catch { $report.rememberedFolderCleanupFailure = $_.Exception.Message }
    [System.IO.File]::WriteAllText($reportPath, ($report | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
    foreach ($process in $ownedProcesses) { $process.Dispose() }
}
$report.ReportPath = $reportPath
[pscustomobject]$report
