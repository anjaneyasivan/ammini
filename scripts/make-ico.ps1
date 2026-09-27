# Generates a multi-size Windows icon from the app PNG artwork.
#
# The .ico embeds PNG-compressed images (supported by Windows Vista+), so no
# ImageMagick is required. Regenerate after changing assets/ammini-logo.png:
#
#   powershell -ExecutionPolicy Bypass -File scripts/make-ico.ps1
#
# See BUILDING.md ("Regenerating the icon") for how assets/ammini-icon.png is
# derived from assets/ammini-logo.png.

param(
    [string]$Source = (Join-Path $PSScriptRoot "..\assets\ammini-icon.png"),
    [string]$Destination = (Join-Path $PSScriptRoot "..\assets\ammini.ico")
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$sizes = @(256, 128, 64, 48, 32, 16)
$srcImage = [System.Drawing.Image]::FromFile((Resolve-Path -LiteralPath $Source))

$images = @()
foreach ($size in $sizes) {
    $bmp = [System.Drawing.Bitmap]::new($size, $size)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.DrawImage($srcImage, 0, 0, $size, $size)
    $g.Dispose()

    $ms = [System.IO.MemoryStream]::new()
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()

    $images += , @($size, $ms.ToArray())
    $ms.Dispose()
}
$srcImage.Dispose()

$out = [System.IO.MemoryStream]::new()
$writer = [System.IO.BinaryWriter]::new($out)

# ICONDIR header: reserved, type (1 = icon), image count.
$writer.Write([UInt16]0)
$writer.Write([UInt16]1)
$writer.Write([UInt16]$images.Count)

# ICONDIRENTRY per image; 256px is encoded as 0 in the width/height bytes.
$offset = 6 + 16 * $images.Count
foreach ($img in $images) {
    $size = $img[0]
    $data = $img[1]
    $dim = if ($size -ge 256) { 0 } else { $size }
    $writer.Write([Byte]$dim)          # width
    $writer.Write([Byte]$dim)          # height
    $writer.Write([Byte]0)             # palette colors
    $writer.Write([Byte]0)             # reserved
    $writer.Write([UInt16]1)           # color planes
    $writer.Write([UInt16]32)          # bits per pixel
    $writer.Write([UInt32]$data.Length)
    $writer.Write([UInt32]$offset)
    $offset += $data.Length
}

foreach ($img in $images) {
    $writer.Write($img[1])
}
$writer.Flush()

[System.IO.File]::WriteAllBytes($Destination, $out.ToArray())
$writer.Dispose()
$out.Dispose()

Write-Host ("Wrote {0} ({1} bytes, sizes: {2})" -f $Destination, (Get-Item $Destination).Length, ($sizes -join ", "))
