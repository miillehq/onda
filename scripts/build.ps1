param([switch]$WithVulkan, [switch]$SignedUpdates)
$ErrorActionPreference = 'Stop'
$repositoryDirectory = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repositoryDirectory
try {
    & (Join-Path $PSScriptRoot 'prepare.ps1') -Release -WithVulkan:$WithVulkan
    npm run contracts
    if ($LASTEXITCODE -ne 0) { throw 'Contract generation failed.' }
    node scripts/generate-notices.mjs
    if ($LASTEXITCODE -ne 0) { throw 'License bundle generation failed.' }
    $configuration = @{ bundle = @{} }
    $runtimeFiles = & (Join-Path $PSScriptRoot 'collect-msvc-runtime.ps1') -WithVulkan:$WithVulkan
    $configuration.bundle.resources = @{
        '../LICENSE' = 'LICENSE'
        '../NOTICE' = 'NOTICE'
        '../THIRD_PARTY_NOTICES.txt' = 'THIRD_PARTY_NOTICES.txt'
    }
    foreach ($runtimeFile in $runtimeFiles.Resources.GetEnumerator()) { $configuration.bundle.resources[$runtimeFile.Key] = $runtimeFile.Value }
    $mplSource = & (Join-Path $PSScriptRoot 'collect-mpl-source.ps1')
    $configuration.bundle.resources[$mplSource] = 'licenses/MPL-source.zip'
    if ($env:ONDA_WINDOWS_SIGN_COMMAND) { $configuration.bundle.windows = @{ signCommand = $env:ONDA_WINDOWS_SIGN_COMMAND } }
    if ($env:ONDA_WINDOWS_CERT_THUMBPRINT) {
        $configuration.bundle.windows = @{ certificateThumbprint=$env:ONDA_WINDOWS_CERT_THUMBPRINT; digestAlgorithm='sha256'; timestampUrl='http://timestamp.digicert.com' }
        $helpers = @('onda-input','onda-asr-worker')
        if ($WithVulkan) { $helpers += 'onda-asr-vulkan-worker' }
        foreach($helper in $helpers) { & (Join-Path $PSScriptRoot 'sign-windows.ps1') -Path (Join-Path $repositoryDirectory "src-tauri/binaries/$helper-x86_64-pc-windows-msvc.exe") }
    }
    if ($WithVulkan) { $configuration.bundle.externalBin = @('binaries/onda-input', 'binaries/onda-asr-worker', 'binaries/onda-asr-vulkan-worker') }
    if ($SignedUpdates) {
        foreach ($secretName in @('TAURI_SIGNING_PRIVATE_KEY','ONDA_UPDATE_ENDPOINT','ONDA_UPDATE_PUBLIC_KEY')) { if (-not [Environment]::GetEnvironmentVariable($secretName)) { throw "Missing release configuration: $secretName" } }
        $configuration.bundle.createUpdaterArtifacts = $true
    }
    $overlayDirectory = Join-Path $repositoryDirectory '.artifacts'
    [System.IO.Directory]::CreateDirectory($overlayDirectory) | Out-Null
    $overlayPath = Join-Path $overlayDirectory 'bundle-config.json'
    [System.IO.File]::WriteAllText($overlayPath, ($configuration | ConvertTo-Json -Depth 6))
    npm run tauri build -- --config $overlayPath
    if ($LASTEXITCODE -ne 0) { throw 'Windows installer build failed.' }
    $installerDirectory = Join-Path $repositoryDirectory 'target/release/bundle/nsis'
    Get-ChildItem -LiteralPath $installerDirectory -Filter '*setup.exe' | ForEach-Object {
        $digest = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        [System.IO.File]::WriteAllText((Join-Path $installerDirectory "$($_.Name).sha256"), "$digest  $($_.Name)`n")
    }
} finally { Pop-Location }
