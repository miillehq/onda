param([Parameter(Mandatory)][string]$ControlRun,[Parameter(Mandatory)][string]$CandidateRun,[Parameter(Mandatory)][int]$Sample)
$ErrorActionPreference='Stop'
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') {throw 'Requires a disposable hosted Windows machine.'}
$root = [IO.Path]::GetFullPath((Join-Path $env:GITHUB_WORKSPACE '.artifacts'))
$parent = (Get-Item -LiteralPath $env:LOCALAPPDATA).FullName
$profileName = 'com.miille.onda'
$ownedProfile = [IO.Path]::GetFullPath((Join-Path $parent $profileName))
$ramTool = Join-Path $env:RUNNER_TEMP 'onda-rammap/RAMMap64.exe'
$sign = Get-AuthenticodeSignature -LiteralPath $ramTool
if ($sign.Status -ne 'Valid' -or $sign.SignerCertificate.Subject -notmatch '^CN=Microsoft Corporation,') {throw 'Unexpected memory measurement utility signature.'}
if (Test-Path -LiteralPath $ownedProfile) { throw 'Expected a fresh hosted application profile.' }
$bad=0
$sequence = if (([int][math]::Floor(($Sample-1)/4)+(($Sample-1)%4))%2 -eq 0) { @('control','candidate') } else {@('candidate','control')}
foreach ($variant in $sequence) {
    $folder = Join-Path $env:RUNNER_TEMP ('onda-paired-' + $variant)
    $binaries = @(Get-ChildItem -LiteralPath (Join-Path $folder 'package') -Filter '*x64-setup.exe' -Recurse -File)
    if ($binaries.Count -ne 1) {throw 'Expected a single candidate installer.'}
    $packageHash=(Get-FileHash -LiteralPath $binaries[0].FullName).Hash.ToLowerInvariant()
    $references=@(Get-ChildItem -LiteralPath (Join-Path $folder 'proof') -Recurse -Filter '*.json' -File | ForEach-Object {(Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json).installerSha256})
    if ($packageHash -notin $references) {throw 'The installer differs from its native report.'}
    $env:ONDA_STUDY_VARIANT=$variant
    $env:ONDA_STUDY_SAMPLE=$Sample
    $env:ONDA_STUDY_POSITION=[Array]::IndexOf($sequence,$variant)+1
    $env:ONDA_STUDY_LOAD_PID=$null
    $null=New-Item -ItemType Directory -Path $ownedProfile -ErrorAction Stop
    $reset=Start-Process -FilePath $ramTool -ArgumentList @('-accepteula','-Et') -WindowStyle Hidden -PassThru
    if (-not $reset.WaitForExit(30000)) {throw 'The standby-cache control did not complete.'}
    if ($reset.ExitCode -ne 0) {throw 'Standby-cache control returned failure.'}
    try {
        ./scripts/startup-study/run.ps1 $binaries[0].FullName
    } catch {
        Write-Warning ('Preserving this attempt failure: ' + $_.Exception.Message)
        $bad++
    }
    if ($env:ONDA_STUDY_LOAD_PID) {
        $load = Get-Process -Id ([int]$env:ONDA_STUDY_LOAD_PID) -ErrorAction SilentlyContinue
        if ($load -and -not $load.WaitForExit(60000)) {throw 'Local workload unexpectedly outlived its duration.'}
    }
    $bucket=Join-Path $root ('case-' + $variant)
    $null=New-Item -ItemType Directory -Path $bucket -ErrorAction Stop
    foreach ($name in @('installed-smoke','startup-study')) {
        $path=Get-Item -LiteralPath (Join-Path $root $name)
        if ($path.Parent.FullName -ne $root -or ($path.Attributes -band [IO.FileAttributes]::ReparsePoint)) {throw 'Unexpected measurement directory.'}
        Move-Item -LiteralPath $path.FullName -Destination (Join-Path $bucket $name) -ErrorAction Stop
    }
    $profileInfo=Get-Item -LiteralPath $ownedProfile
    if ($profileInfo.Parent.FullName -ne $parent -or $profileInfo.Name -ne $profileName -or ($profileInfo.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unexpected hosted profile ownership.' }
    Rename-Item -LiteralPath $ownedProfile -NewName ($profileName+'.paired-'+$variant) -ErrorAction Stop
}
if ($bad -gt 0) {throw "$bad cold launch(es) failed the original check."}
