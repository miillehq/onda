$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$iconDirectory = Join-Path $PSScriptRoot '../src-tauri/icons'
[System.IO.Directory]::CreateDirectory($iconDirectory) | Out-Null
foreach ($iconSize in @(32, 128, 256)) {
    $bitmap = [System.Drawing.Bitmap]::new($iconSize, $iconSize)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.Clear([System.Drawing.Color]::FromArgb(20, 32, 49))
    $brush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(112, 220, 201))
    $unit = $iconSize / 8
    $graphics.FillRectangle($brush, [float](2*$unit), [float](1.5*$unit), [float]$unit, [float](5*$unit))
    foreach ($row in @(1.5, 3.5, 5.5)) { $graphics.FillRectangle($brush, [float](2*$unit), [float]($row*$unit), [float](4*$unit), [float]$unit) }
    $bitmap.Save((Join-Path $iconDirectory "$($iconSize)x$($iconSize).png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $brush.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
}
$pngBytes = [System.IO.File]::ReadAllBytes((Join-Path $iconDirectory '256x256.png'))
$iconStream = [System.IO.File]::Create((Join-Path $iconDirectory 'icon.ico'))
$iconWriter = [System.IO.BinaryWriter]::new($iconStream)
$iconWriter.Write([uint16]0); $iconWriter.Write([uint16]1); $iconWriter.Write([uint16]1)
$iconWriter.Write([byte]0); $iconWriter.Write([byte]0); $iconWriter.Write([byte]0); $iconWriter.Write([byte]0)
$iconWriter.Write([uint16]1); $iconWriter.Write([uint16]32); $iconWriter.Write([uint32]$pngBytes.Length); $iconWriter.Write([uint32]22)
$iconWriter.Write($pngBytes); $iconWriter.Dispose()
