param([Parameter(Mandatory=$true)][string]$Path, [string]$CertificateThumbprint=$env:ONDA_WINDOWS_CERT_THUMBPRINT)
$ErrorActionPreference='Stop'
if ($CertificateThumbprint -notmatch '^[0-9a-fA-F]{40}$') { throw 'Set ONDA_WINDOWS_CERT_THUMBPRINT to a valid publisher certificate thumbprint in the Windows certificate store.' }
$signedPath=(Resolve-Path -LiteralPath $Path -ErrorAction Stop).Path
$sdkDirectory=Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$signTool=Get-ChildItem -LiteralPath $sdkDirectory -Directory | Sort-Object Name -Descending | ForEach-Object { Join-Path $_.FullName 'x64/signtool.exe' } | Where-Object {Test-Path -LiteralPath $_ -PathType Leaf} | Select-Object -First 1
if(-not $signTool){throw 'The Windows SDK x64 signtool.exe is required for Authenticode signing.'}
& $signTool sign /sha1 $CertificateThumbprint /fd SHA256 /tr 'http://timestamp.digicert.com' /td SHA256 $signedPath
if($LASTEXITCODE -ne 0){throw 'Publisher Authenticode signing failed.'}
if((Get-AuthenticodeSignature -LiteralPath $signedPath).Status -ne 'Valid'){throw 'The publisher signature did not validate after signing.'}
