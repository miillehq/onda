<#
.SYNOPSIS
Bundle the exact reviewed, unmodified MPL dependencies from locked Cargo metadata.
.DESCRIPTION
Returns only the absolute .artifacts/MPL-source.zip path. The ZIP belongs in installer
resources as licenses/MPL-source.zip, so local as well as CI distributions carry source access.
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
$artifactDirectory = [System.IO.Path]::GetFullPath((Join-Path $repositoryDirectory '.artifacts'))
$approvedPackages = @{
    'cssparser' = '0.37.0'
    'cssparser-macros' = '0.7.1'
    'dtoa-short' = '0.3.5'
    'selectors' = '0.38.0'
    'option-ext' = '0.2.0'
}
$stagingDirectory = Join-Path $artifactDirectory ("mpl-source-" + [guid]::NewGuid().ToString('N'))
$archivePath = Join-Path $artifactDirectory 'MPL-source.zip'
[System.IO.Directory]::CreateDirectory($stagingDirectory) | Out-Null

Push-Location -LiteralPath $repositoryDirectory
try {
    $metadataText = (& cargo metadata --locked --format-version 1) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw 'Locked Cargo metadata failed; MPL sources cannot be bundled.' }
    $metadata = $metadataText | ConvertFrom-Json
    $packages = @($metadata.packages | Where-Object license -EQ 'MPL-2.0' | Sort-Object name,version)
    if ($packages.Count -eq 0) { throw 'The expected reviewed MPL source boundary is absent; review the lockfile before distribution.' }
    $inventory = [System.Collections.Generic.List[object]]::new()
    foreach ($package in $packages) {
        if (-not $approvedPackages.ContainsKey($package.name) -or $approvedPackages[$package.name] -ne $package.version) {
            throw "Unreviewed MPL package/version: $($package.name) $($package.version). Update the license decision and deny policy after review."
        }
        $sourceDirectory = Split-Path -Parent $package.manifest_path
        $destination = Join-Path $stagingDirectory "$($package.name)-$($package.version)"
        Copy-Item -LiteralPath $sourceDirectory -Destination $destination -Recurse
        $inventory.Add([ordered]@{ name = $package.name; version = $package.version; license = 'MPL-2.0'; source = $package.source })
    }
    $readme = @'
Corresponding MPL dependency source for Eagle

These directories contain unmodified package sources from the exact Cargo.lock revisions.
They remain available under the Mozilla Public License 2.0; see each package's notices/license.
Eagle's own separate source files are licensed Apache-2.0. This ZIP supplies the MPL source
associated with the distributed runtime/build dependencies and must remain available to recipients.
'@
    [System.IO.File]::WriteAllText((Join-Path $stagingDirectory 'README.txt'), $readme, [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $stagingDirectory 'SOURCE-MANIFEST.json'), ($inventory.ToArray() | ConvertTo-Json -Depth 4), [System.Text.UTF8Encoding]::new($false))
    # Stable entry order/timestamps avoid random staging names or registry mtimes in release bytes.
    $zipEpoch = [System.DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [System.TimeSpan]::Zero)
    $archiveStream = [System.IO.File]::Open($archivePath, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
    $archive = [System.IO.Compression.ZipArchive]::new($archiveStream, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($file in Get-ChildItem -LiteralPath $stagingDirectory -File -Recurse | Sort-Object FullName) {
            $relative = $file.FullName.Substring($stagingDirectory.Length + 1).Replace('\', '/')
            $entry = $archive.CreateEntry($relative, [System.IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = $zipEpoch
            $inputStream = [System.IO.File]::OpenRead($file.FullName)
            $outputStream = $entry.Open()
            try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose(); $inputStream.Dispose() }
        }
    } finally { $archive.Dispose(); $archiveStream.Dispose() }
    return $archivePath
} finally {
    Pop-Location
    # A freshly generated directory is removed only after its absolute path is checked.
    $resolvedStagingDirectory = [System.IO.Path]::GetFullPath($stagingDirectory)
    $allowedPrefix = $artifactDirectory.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $resolvedStagingDirectory.StartsWith($allowedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing MPL staging cleanup outside the artifact directory.' }
    if (Test-Path -LiteralPath $resolvedStagingDirectory) { Remove-Item -LiteralPath $resolvedStagingDirectory -Recurse -Force }
}
