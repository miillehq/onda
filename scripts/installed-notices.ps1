# Checks that an installed Windows package carries Onda's own licenses, the MPL source and every
# license file its Microsoft runtime manifest lists. installer-smoke.ps1 dot-sources it.
function Assert-InstalledNotices {
    param(
        [Parameter(Mandatory = $true)][string]$InstallDirectory,
        [Parameter(Mandatory = $true)]$Manifest
    )
    $noticeProperty = $Manifest.PSObject.Properties['notices']
    [string[]]$runtimeNotices = if ($noticeProperty) { @($noticeProperty.Value) } else { @() }
    if (-not ($runtimeNotices -like 'licenses/Microsoft-Visual-Studio-*-Redist.txt')) { throw 'The runtime manifest names no Microsoft distributable list.' }
    [string[]]$required = @('LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.txt', 'licenses/MPL-source.zip') + $runtimeNotices
    # A Visual Studio 2022 runtime comes with Microsoft's two license documents.
    $visualStudioProperty = $Manifest.PSObject.Properties['visualStudio']
    if ($visualStudioProperty -and $visualStudioProperty.Value -eq '2022') {
        $required += @('licenses/Microsoft-Visual-Cpp-Runtime-2015-2022.docx', 'licenses/Microsoft-Build-Tools-2022.docx')
    }
    foreach ($name in $required) {
        if (-not (Test-Path -LiteralPath (Join-Path $InstallDirectory $name) -PathType Leaf)) { throw "Installed license/source resource missing: $name" }
    }
}
