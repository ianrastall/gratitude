# Draws the application icon and writes it as a multi-size .ico.
# The result is committed; run this only to change the design.
# Usage: .\tools\make_icon.ps1
Add-Type -AssemblyName System.Drawing

$out = Join-Path $PSScriptRoot '..\src\ShowPlayer.App\Assets\AppIcon.ico'
New-Item -ItemType Directory -Force (Split-Path $out) | Out-Null

function New-IconPng([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.PixelOffsetMode = 'HighQuality'
    $g.ScaleTransform($size / 256.0, $size / 256.0)

    # Rounded dark tile.
    $tile = New-Object System.Drawing.Drawing2D.GraphicsPath
    $r = 52
    $tile.AddArc(8, 8, $r * 2, $r * 2, 180, 90)
    $tile.AddArc(248 - $r * 2, 8, $r * 2, $r * 2, 270, 90)
    $tile.AddArc(248 - $r * 2, 248 - $r * 2, $r * 2, $r * 2, 0, 90)
    $tile.AddArc(8, 248 - $r * 2, $r * 2, $r * 2, 90, 90)
    $tile.CloseFigure()
    $g.FillPath((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 30, 34, 44))), $tile)

    # Three book spines on a shelf.
    $spines = @(
        @{ X = 50;  Y = 62; W = 40; H = 126; C = [System.Drawing.Color]::FromArgb(255, 232, 168, 56) },
        @{ X = 98;  Y = 46; W = 46; H = 142; C = [System.Drawing.Color]::FromArgb(255, 86, 156, 214) },
        @{ X = 152; Y = 74; W = 36; H = 114; C = [System.Drawing.Color]::FromArgb(255, 214, 96, 96) })
    foreach ($s in $spines) {
        $g.FillRectangle((New-Object System.Drawing.SolidBrush $s.C), $s.X, $s.Y, $s.W, $s.H)
        $band = [System.Drawing.Color]::FromArgb(70, 255, 255, 255)
        $g.FillRectangle((New-Object System.Drawing.SolidBrush $band), $s.X, $s.Y + 16, $s.W, 8)
    }
    $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 222, 226, 234))), 38, 188, 180, 12)

    # Play badge.
    $g.FillEllipse((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 30, 34, 44))), 142, 142, 96, 96)
    $g.FillEllipse((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 245, 247, 250))), 150, 150, 80, 80)
    $play = [System.Drawing.PointF[]]@(
        (New-Object System.Drawing.PointF 178, 170),
        (New-Object System.Drawing.PointF 178, 210),
        (New-Object System.Drawing.PointF 212, 190))
    $g.FillPolygon((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 30, 34, 44))), $play)

    $g.Dispose()
    $stream = New-Object System.IO.MemoryStream
    $bmp.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    , $stream.ToArray()
}

$sizes = 16, 20, 24, 32, 40, 48, 64, 256
$images = foreach ($size in $sizes) { , (New-IconPng $size) }

# ICO container: a directory of PNG-compressed images.
$file = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $file
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dimension = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$dimension); $w.Write([byte]$dimension); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$images[$i].Length); $w.Write([uint32]$offset)
    $offset += $images[$i].Length
}
foreach ($image in $images) { $w.Write($image) }
$w.Flush()
[System.IO.File]::WriteAllBytes($out, $file.ToArray())
"wrote $out ($($file.Length) bytes)"
