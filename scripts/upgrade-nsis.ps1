param(
    [Parameter(Mandatory)][ValidateSet('install', 'uninstall')][string]$Action,
    [Parameter(Mandatory)][string]$InstallDirectory,
    [string]$InstallerPath
)
$ErrorActionPreference = 'Stop'
$owned = [IO.Path]::GetFullPath($InstallDirectory)
$runnerRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\') + '\'
$parent = [IO.Directory]::GetParent($owned)
if ($env:DISPOSABLE_RUNNER -ne 'github-hosted' -or -not $owned.StartsWith($runnerRoot, [StringComparison]::OrdinalIgnoreCase) -or
    $parent.Name -notlike 'onda-upgrade-*' -or [IO.Path]::GetFileName($owned) -ne 'install') {
    throw 'Expected the test installation under the disposable runner directory.'
}
if ($Action -eq 'install') {
    $program = (Get-Item -LiteralPath $InstallerPath).FullName
    $arguments = @('/S', "/D=$owned")
} else {
    $program = Join-Path $owned 'uninstall.exe'
    $arguments = @('/S')
}
# NSIS can hand cleanup to a temporary child and exit. PowerShell waits for the process tree.
$process = Start-Process -FilePath $program -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "NSIS $Action exited with $($process.ExitCode)." }
