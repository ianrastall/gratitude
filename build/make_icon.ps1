# Draws the application icon and writes it as a multi-size .ico.
# The result is committed; run this only to change the design.
# The rose is a generic flower drawn here, not taken from any band artwork.
# Usage: pwsh build/make_icon.ps1 [preview.png]
param([string]$Preview)
Add-Type -AssemblyName System.Drawing

$out = Join-Path $PSScriptRoot '..\src\ShowPlayer.App\Assets\AppIcon.ico'
New-Item -ItemType Directory -Force (Split-Path $out) | Out-Null

function Get-Color([int]$r, [int]$g, [int]$b) { [System.Drawing.Color]::FromArgb(255, $r, $g, $b) }

function Add-Petals($g, [double]$cx, [double]$cy, [int]$count, [double]$reach, [double]$radius, [double]$turn, $fill, $edge, [float]$line) {
    $brush = New-Object System.Drawing.SolidBrush $fill
    $pen = New-Object System.Drawing.Pen $edge, $line
    for ($i = 0; $i -lt $count; $i++) {
        $angle = ($turn + 360.0 * $i / $count) * [math]::PI / 180
        $x = $cx + $reach * [math]::Cos($angle) - $radius
        $y = $cy + $reach * [math]::Sin($angle) - $radius
        $g.FillEllipse($brush, [float]$x, [float]$y, [float]($radius * 2), [float]($radius * 2))
        $g.DrawEllipse($pen, [float]$x, [float]$y, [float]($radius * 2), [float]($radius * 2))
    }
}

function New-IconBitmap([int]$size) {
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
    $g.FillPath((New-Object System.Drawing.SolidBrush (Get-Color 30 34 44)), $tile)

    # Two leaves behind the bloom.
    foreach ($leaf in @(@{ Angle = 148; Fill = (Get-Color 58 122 76) }, @{ Angle = 32; Fill = (Get-Color 46 104 64) })) {
        $state = $g.Save()
        $g.TranslateTransform(128, 150)
        $g.RotateTransform($leaf.Angle)
        $path = New-Object System.Drawing.Drawing2D.GraphicsPath
        $path.AddBezier(30, 0, 62, -34, 92, -30, 112, 0)
        $path.AddBezier(112, 0, 92, 30, 62, 34, 30, 0)
        $g.FillPath((New-Object System.Drawing.SolidBrush $leaf.Fill), $path)
        $g.Restore($state)
    }

    # The bloom, seen from above: rings of overlapping petals around a tight center.
    $edge = Get-Color 92 12 34
    Add-Petals $g 128 116 5 46 44 -90 (Get-Color 170 26 52) $edge 5
    Add-Petals $g 128 116 5 25 32 -54 (Get-Color 196 38 66) $edge 5
    Add-Petals $g 128 116 3 12 21 -30 (Get-Color 224 66 88) $edge 5

    # The center spiral.
    $spiral = New-Object System.Drawing.Pen $edge, 7
    $spiral.StartCap = 'Round'
    $spiral.EndCap = 'Round'
    $points = for ($t = 0; $t -le 380; $t += 20) {
        $reach = 2 + $t * 0.042
        $angle = $t * [math]::PI / 180
        New-Object System.Drawing.PointF ([float](128 + $reach * [math]::Cos($angle))), ([float](116 + $reach * [math]::Sin($angle)))
    }
    $g.DrawCurve($spiral, [System.Drawing.PointF[]]$points)

    $g.Dispose()
    $bmp
}

function New-IconPng([int]$size) {
    $bmp = New-IconBitmap $size
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

if ($Preview) {
    # Every size enlarged without smoothing, on the dark and light title bar colors.
    $sheet = New-Object System.Drawing.Bitmap 1500, 720
    $g = [System.Drawing.Graphics]::FromImage($sheet)
    $g.FillRectangle((New-Object System.Drawing.SolidBrush (Get-Color 32 32 32)), 0, 0, 1500, 360)
    $g.FillRectangle((New-Object System.Drawing.SolidBrush (Get-Color 243 241 236)), 0, 360, 1500, 360)
    $g.InterpolationMode = 'NearestNeighbor'
    $g.PixelOffsetMode = 'Half'
    foreach ($band in 0, 360) {
        $x = 16
        foreach ($size in 16, 24, 32, 48) {
            $icon = New-IconBitmap $size
            $g.DrawImage($icon, $x, $band + 16, 224, 224)
            $g.DrawImage($icon, $x, $band + 256, $size, $size)
            $icon.Dispose()
            $x += 240
        }
        $icon = New-IconBitmap 256
        $g.DrawImage($icon, $x + 8, $band + 16, 256, 256)
        $icon.Dispose()
    }
    $g.Dispose()
    $sheet.Save($Preview, [System.Drawing.Imaging.ImageFormat]::Png)
    $sheet.Dispose()
}
