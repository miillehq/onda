$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'prepare.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Helper preparation failed.' }
npm run tauri dev
if ($LASTEXITCODE -ne 0) { throw 'Desktop development process failed.' }
