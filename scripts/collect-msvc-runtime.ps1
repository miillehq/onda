<#
.SYNOPSIS
Collect unmodified Microsoft release CRT files for the host architecture (x64 or ARM64) and their
license evidence for app-local deployment.
.DESCRIPTION
Returns one object containing Resources (absolute source -> installer-relative path), Files,
ManifestPath and AuditedExecutables. No DLLs are taken from System32 or debug_nonredist.
The publisher must have redistribution rights under its applicable Visual Studio license.
.PARAMETER ExecutablePaths
Optional final executables to inspect with dumpbin. Otherwise existing packaged release workers/shell are inspected.
.PARAMETER OutputDirectory
Ignored build-artifact directory. Defaults to the repository's .artifacts/runtime.
.PARAMETER WithVulkan
Audit the separately named Vulkan worker when present and include SDK license/notice evidence.
#>
[CmdletBinding()]
param(
    [string[]]$ExecutablePaths = @(),
    [string]$OutputDirectory = '',
    [switch]$WithVulkan
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
# The installer carries the runtime of the machine that builds it: x64 on x64, ARM64 on Windows on ARM.
$architecture = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
$visualStudioComponent = @{ x64 = 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64'; arm64 = 'Microsoft.VisualStudio.Component.VC.Tools.ARM64' }[$architecture]
$machinePattern = @{ x64 = '\b8664 machine \(x64\)'; arm64 = '\bAA64 machine \(ARM64\)' }[$architecture]
# vcruntime140_1.dll carries x64's exception handling; the dependency closure below adds any
# other runtime DLL a worker imports.
$mandatoryRuntimeNames = @{ x64 = @('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'); arm64 = @('msvcp140.dll', 'vcruntime140.dll') }[$architecture]
# Visual Studio 2022 where it is installed: the x64 runners and most developer machines. Since
# September 2026 GitHub's Windows on ARM runners have only Visual Studio 2026, whose release runtime
# folder is Microsoft.VC145.CRT; its DLLs keep the 140 names.
$visualStudioProducts = @(
    [pscustomobject]@{ Year = '2022'; VersionRange = '[17.0,18.0)' },
    [pscustomobject]@{ Year = '2026'; VersionRange = '[18.0,19.0)' }
)
$redistributionReferences = @{
    '2022' = 'https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution'
    '2026' = 'https://learn.microsoft.com/en-us/visualstudio/releases/2026/redistribution'
}
# Microsoft publishes the 2022 runtime terms as a document, carried below with a pinned hash, and
# the 2026 terms only as a web page, which the manifest names.
$runtimeLicenseTerms = @{
    '2022' = 'licenses/Microsoft-Visual-Cpp-Runtime-2015-2022.docx'
    '2026' = 'https://visualstudio.microsoft.com/license-terms/vs2026-ga-visualcpp-v14-redist-runtime/'
}

function Get-VisualStudioInstallation {
    <# Resolve the installed C++ toolchain using Microsoft's discovery utility, 2022 before 2026. #>
    $locator = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $locator)) { throw 'vswhere.exe is required to locate licensed Visual Studio C++ redistributables.' }
    foreach ($product in $visualStudioProducts) {
        $installation = & $locator -latest -version $product.VersionRange -products '*' -requires $visualStudioComponent -property installationPath
        if ($LASTEXITCODE -eq 0 -and $installation) { return [pscustomobject]@{ Path = [System.IO.Path]::GetFullPath([string]$installation); Year = $product.Year } }
    }
    throw "Visual Studio 2022 or 2026 $architecture C++ tools and redistributable component are required."
}

function Find-VisualStudioNotice {
    <# The installation's English notice; one found elsewhere under Licenses is used only if unique. #>
    param([string]$Name)
    $licenseDirectory = Join-Path $visualStudioDirectory 'Licenses'
    $preferred = Join-Path $licenseDirectory "1033/$Name"
    if (Test-Path -LiteralPath $preferred -PathType Leaf) { return $preferred }
    $found = @(Get-ChildItem -LiteralPath $licenseDirectory -File -Recurse -Filter $Name -ErrorAction SilentlyContinue)
    if ($found.Count -eq 1) { return $found[0].FullName }
    $available = @(Get-ChildItem -LiteralPath $licenseDirectory -File -Recurse -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName.Substring($licenseDirectory.Length + 1) })
    throw "Visual Studio $visualStudioYear has no single Licenses notice $Name; it has: $($available -join ', ')"
}

function Get-LatestVersionDirectory {
    <# Numeric version ordering avoids selecting alias/debug directories. #>
    param([string]$Directory)
    $versions = @(Get-ChildItem -LiteralPath $Directory -Directory | Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } | Sort-Object { [version]$_.Name } -Descending)
    if ($versions.Count -eq 0) { throw 'Microsoft C++ release component version directory is unavailable.' }
    return $versions[0].FullName
}

function Assert-MicrosoftReleaseBinary {
    <# Authenticity and architecture are checked before any file is bundled. #>
    param([string]$Path, [string]$Dumpbin)
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or -not $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)') {
        throw "Microsoft runtime signature verification failed: $([System.IO.Path]::GetFileName($Path))"
    }
    $headers = & $Dumpbin /nologo /headers $Path
    if ($LASTEXITCODE -ne 0 -or -not ($headers -match $machinePattern)) { throw "Microsoft runtime DLL is not a verified $architecture PE image." }
}

function Get-ImportedDllNames {
    <# Time O(n) dumpbin output; space O(k) distinct direct imports. #>
    param([string]$Path, [string]$Dumpbin)
    $output = & $Dumpbin /nologo /dependents $Path
    if ($LASTEXITCODE -ne 0) { throw "Dependency inspection failed: $([System.IO.Path]::GetFileName($Path))" }
    return @($output | ForEach-Object { if ($_ -match '^\s+([A-Za-z0-9_.-]+\.dll)\s*$') { $Matches[1].ToLowerInvariant() } } | Sort-Object -Unique)
}

function Copy-Notice {
    param([string]$Source, [string]$Name, [System.Collections.IDictionary]$Resources, [string]$NoticeDirectory)
    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) { throw "Required Microsoft notice missing: $Name" }
    $destination = Join-Path $NoticeDirectory $Name
    Copy-Item -LiteralPath $Source -Destination $destination -Force
    $Resources[$destination] = "licenses/$Name"
}

function Get-VerifiedLicenseDocument {
    <# Official unchanged license documents are cached with pinned SHA-256, not regenerated prose. #>
    param([string]$Url, [string]$Name, [string]$ExpectedSha256, [System.Collections.IDictionary]$Resources, [string]$NoticeDirectory)
    $destination = Join-Path $NoticeDirectory $Name
    if (-not (Test-Path -LiteralPath $destination -PathType Leaf)) { Invoke-WebRequest -Uri $Url -OutFile $destination -UseBasicParsing -TimeoutSec 90 }
    $actual = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $ExpectedSha256) { throw "Official Microsoft license document changed; review before release: $Name" }
    $Resources[$destination] = "licenses/$Name"
}

function Assert-DependencyClosure {
    <# Time O(v + e) inspected imports; space O(v + e), each packaged DLL inspected once. #>
    param([string[]]$Executables, [hashtable]$RuntimeByName, [string]$Dumpbin)
    $queue = [System.Collections.Generic.Queue[string]]::new()
    $visited = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    $inspections = [System.Collections.Generic.List[object]]::new()
    foreach ($executable in $Executables) {
        if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'An explicitly requested packaged executable is missing.' }
        $queue.Enqueue([System.IO.Path]::GetFullPath($executable))
    }
    while ($queue.Count -gt 0) {
        $path = $queue.Dequeue()
        if (-not $visited.Add($path)) { continue }
        $dependencies = @(Get-ImportedDllNames -Path $path -Dumpbin $Dumpbin)
        $resolved = [System.Collections.Generic.List[object]]::new()
        foreach ($name in $dependencies) {
            if ($name -match '^(?:msvcp|vcruntime)\d+.*d\.dll$|^ucrtbased\.dll$') { throw "Debug runtime cannot be distributed: $name" }
            if ($RuntimeByName.ContainsKey($name)) {
                $queue.Enqueue($RuntimeByName[$name])
                $resolved.Add([ordered]@{ name = $name; resolution = 'appLocalMicrosoftRuntime' })
                continue
            }
            if ($WithVulkan -and $name -eq 'vulkan-1.dll' -and [System.IO.Path]::GetFileName($path) -match 'vulkan') {
                $resolved.Add([ordered]@{ name = $name; resolution = 'optionalVendorVulkanDriver' })
                continue
            }
            if ($name -match '^(?:api-ms-win-|ext-ms-win-)' -or (Test-Path -LiteralPath (Join-Path $env:SystemRoot "System32/$name") -PathType Leaf)) {
                # API sets and Windows UCRT are system components on the Windows 10+ support floor.
                if ($name -match '^(?:msvcp|vcruntime|concrt|vccorlib)') { throw "Unbundled Microsoft runtime dependency: $name" }
                $resolved.Add([ordered]@{ name = $name; resolution = 'windowsSystem' })
                continue
            }
            throw "Unresolved non-system packaged dependency: $name"
        }
        $inspections.Add([ordered]@{ binary = [System.IO.Path]::GetFileName($path); imports = $resolved.ToArray() })
    }
    return ,$inspections.ToArray()
}

function Copy-VulkanHeaderEvidence {
    <# Preserve notices from the actual headers used, even in a portable SDK extraction. #>
    param([string]$SdkDirectory, [System.Collections.IDictionary]$Resources, [string]$NoticeDirectory)
    $includeDirectory = Join-Path $SdkDirectory 'Include'
    if (-not (Test-Path -LiteralPath (Join-Path $includeDirectory 'vulkan/vulkan.h'))) { throw 'The selected Vulkan SDK header root is missing.' }
    $notices = [System.Collections.Generic.List[string]]::new()
    $notices.Add('Vulkan headers used by Onda optional acceleration; SDK compiler/tools are not redistributed.')
    $notices.Add('Per-file copyright and SPDX notices follow from the actual installed header bytes. Apache-2.0 OR MIT files may be used under Apache-2.0; the complete Apache text accompanies this notice.')
    foreach ($directory in @('vulkan', 'vk_video')) {
        $sourceDirectory = Join-Path $includeDirectory $directory
        if (-not (Test-Path -LiteralPath $sourceDirectory)) { continue }
        foreach ($header in Get-ChildItem -LiteralPath $sourceDirectory -File -Recurse | Where-Object Extension -In '.h','.hpp') {
            $licenseLines = @(Get-Content -LiteralPath $header.FullName -TotalCount 32 | Where-Object { $_ -match 'Copyright|SPDX-License-Identifier|Licensed under the Apache License, Version 2.0' })
            if (-not ($licenseLines -match 'SPDX-License-Identifier: (?:Apache-2.0(?: OR MIT)?|MIT)|Licensed under the Apache License, Version 2.0')) { throw "Unreviewed Vulkan header license: $($header.Name)" }
            $relative = $header.FullName.Substring($includeDirectory.TrimEnd([System.IO.Path]::DirectorySeparatorChar).Length + 1)
            $hash = (Get-FileHash -LiteralPath $header.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            $notices.Add("`n$relative`nSHA-256: $hash")
            foreach ($line in $licenseLines) { $notices.Add($line) }
        }
    }
    $noticePath = Join-Path $NoticeDirectory 'Vulkan-Headers-Notices.txt'
    [System.IO.File]::WriteAllText($noticePath, ($notices -join "`n"), [System.Text.UTF8Encoding]::new($false))
    $Resources[$noticePath] = 'licenses/Vulkan-Headers-Notices.txt'
    Copy-Notice -Source (Join-Path $repositoryDirectory 'LICENSE') -Name 'Vulkan-Headers-Apache-2.0.txt' -Resources $Resources -NoticeDirectory $NoticeDirectory
    $mitText = @'
MIT License

Copyright notices are preserved per header in Vulkan-Headers-Notices.txt.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
'@
    $mitPath = Join-Path $NoticeDirectory 'Vulkan-Headers-MIT.txt'
    [System.IO.File]::WriteAllText($mitPath, $mitText, [System.Text.UTF8Encoding]::new($false))
    $Resources[$mitPath] = 'licenses/Vulkan-Headers-MIT.txt'
}

$visualStudio = Get-VisualStudioInstallation
$visualStudioDirectory = $visualStudio.Path
$visualStudioYear = $visualStudio.Year
$redistVersionDirectory = Get-LatestVersionDirectory -Directory (Join-Path $visualStudioDirectory 'VC/Redist/MSVC')
$redistVersion = [System.IO.Path]::GetFileName($redistVersionDirectory)
# The release runtime folder is named after the toolset: Microsoft.VC143.CRT in 2022, VC145 in 2026.
$runtimeDirectories = @(Get-ChildItem -LiteralPath (Join-Path $redistVersionDirectory $architecture) -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^Microsoft\.VC\d+\.CRT$' })
if ($runtimeDirectories.Count -ne 1) { throw "The $architecture Microsoft.VC*.CRT release redistributable component is missing or ambiguous in VC/Redist/MSVC/$redistVersion." }
$runtimeDirectory = $runtimeDirectories[0].FullName
$runtimeFolderName = $runtimeDirectories[0].Name
$toolsetDirectory = Get-LatestVersionDirectory -Directory (Join-Path $visualStudioDirectory 'VC/Tools/MSVC')
# dumpbin reads either architecture's images; on Windows on ARM prefer the native one.
$dumpbinCandidates = if ($architecture -eq 'arm64') { @('bin/Hostarm64/arm64/dumpbin.exe', 'bin/Hostx64/x64/dumpbin.exe') } else { @('bin/Hostx64/x64/dumpbin.exe') }
$dumpbin = $dumpbinCandidates | ForEach-Object { Join-Path $toolsetDirectory $_ } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $dumpbin) { throw 'The Microsoft dumpbin.exe dependency inspector is required.' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repositoryDirectory '.artifacts/runtime' }
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
$noticeDirectory = Join-Path $OutputDirectory 'licenses'
[System.IO.Directory]::CreateDirectory($noticeDirectory) | Out-Null
$resources = [ordered]@{}
$files = [System.Collections.Generic.List[object]]::new()
$runtimeByName = @{}

foreach ($mandatoryName in $mandatoryRuntimeNames) {
    if (-not (Test-Path -LiteralPath (Join-Path $runtimeDirectory $mandatoryName) -PathType Leaf)) { throw "Required $architecture release runtime DLL missing: $mandatoryName" }
}
foreach ($file in Get-ChildItem -LiteralPath $runtimeDirectory -File -Filter '*.dll' | Sort-Object Name) {
    Assert-MicrosoftReleaseBinary -Path $file.FullName -Dumpbin $dumpbin
    $version = $file.VersionInfo.FileVersion
    $sourceHash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $destination = Join-Path $OutputDirectory $file.Name
    Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
    if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sourceHash) { throw 'Copied runtime DLL failed integrity verification.' }
    $resources[$destination] = $file.Name
    $runtimeByName[$file.Name.ToLowerInvariant()] = $destination
    $files.Add([ordered]@{ name = $file.Name; version = $version; sha256 = $sourceHash; source = "VC/Redist/MSVC/$redistVersion/$architecture/$runtimeFolderName/$($file.Name)"; publisher = 'Microsoft Corporation' })
}

Copy-Notice -Source (Find-VisualStudioNotice -Name 'Redist.txt') -Name "Microsoft-Visual-Studio-$visualStudioYear-Redist.txt" -Resources $resources -NoticeDirectory $noticeDirectory
Copy-Notice -Source (Find-VisualStudioNotice -Name 'ThirdPartyNotices.txt') -Name "Microsoft-Visual-Studio-$visualStudioYear-ThirdPartyNotices.txt" -Resources $resources -NoticeDirectory $noticeDirectory
if ($visualStudioYear -eq '2022') {
    Get-VerifiedLicenseDocument -Url 'https://visualstudio.microsoft.com/wp-content/uploads/2021/09/Visual-C-Runtime-2015-2022-License-1.docx' -Name 'Microsoft-Visual-Cpp-Runtime-2015-2022.docx' -ExpectedSha256 'f1e3d56ceb2ad68aae0711b910375009e651ac5530fa0760f0dea6e81e54fae1' -Resources $resources -NoticeDirectory $noticeDirectory
    Get-VerifiedLicenseDocument -Url 'https://visualstudio.microsoft.com/wp-content/uploads/2024/03/Visual-Studio-2022-Diagnostic-Build-Tools-Agent-License_Update-March-2024_EN.docx' -Name 'Microsoft-Build-Tools-2022.docx' -ExpectedSha256 '2f66b86a00e8d9833789897ce23d05a4a2dbea370cf39c8c1098dbc17d0e7bdc' -Resources $resources -NoticeDirectory $noticeDirectory
}

if ($WithVulkan) {
    if (-not $env:VULKAN_SDK) { throw 'VULKAN_SDK is required to collect the optional Vulkan build license evidence.' }
    $sdkNotices = @(Get-ChildItem -LiteralPath $env:VULKAN_SDK -File | Where-Object Name -Match 'license|notice')
    foreach ($notice in $sdkNotices) { Copy-Notice -Source $notice.FullName -Name "Vulkan-SDK-$($notice.Name)" -Resources $resources -NoticeDirectory $noticeDirectory }
    Copy-VulkanHeaderEvidence -SdkDirectory $env:VULKAN_SDK -Resources $resources -NoticeDirectory $noticeDirectory
}

if ($ExecutablePaths.Count -eq 0) {
    $binaryNames = @('onda.exe', 'onda-input.exe', 'onda-asr-worker.exe', 'onda-asr-avx2-worker.exe')
    if ($WithVulkan) { $binaryNames += 'onda-asr-vulkan-worker.exe' }
    $ExecutablePaths = @($binaryNames | ForEach-Object { Join-Path $repositoryDirectory "target/release/$_" } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf })
}
if ($ExecutablePaths.Count -eq 0) { throw 'Build the real release workers before collecting their runtime dependency closure.' }
$inspections = Assert-DependencyClosure -Executables $ExecutablePaths -RuntimeByName $runtimeByName -Dumpbin $dumpbin
$manifestPath = Join-Path $OutputDirectory 'Microsoft-runtime-manifest.json'
# Every license file the package carries for the runtime; the installer test checks each is installed.
$notices = @($resources.Values | Where-Object { $_ -like 'licenses/*' })
$manifest = [ordered]@{
    formatVersion = 1
    architecture = $architecture
    visualStudio = $visualStudioYear
    runtimeFolder = $runtimeFolderName
    redistFolderVersion = $redistVersion
    toolsetVersion = [System.IO.Path]::GetFileName($toolsetDirectory)
    license = 'Microsoft Visual Studio / Visual C++ Runtime software license terms; not Apache-2.0'
    runtimeLicenseTerms = $runtimeLicenseTerms[$visualStudioYear]
    redistributionReference = $redistributionReferences[$visualStudioYear]
    notices = $notices
    publisherEligibility = 'Redistribution requires applicable valid Visual Studio licensing; installed Build Tools alone does not prove publisher eligibility.'
    servicing = 'App-local DLLs must be updated and redistributed by the Onda publisher.'
    files = $files.ToArray()
    inspectedImports = $inspections
}
[System.IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
$resources[$manifestPath] = 'licenses/Microsoft-runtime-manifest.json'
[pscustomobject]@{ Resources = $resources; Files = $files.ToArray(); ManifestPath = $manifestPath; AuditedExecutables = @($ExecutablePaths | ForEach-Object { [System.IO.Path]::GetFileName($_) }) }
