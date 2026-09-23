# Renders the SideKit app icon (blue rounded square with the </> chevrons) to PNGs
# and packs a multi-resolution icon.ico. Run from the repo root:
#   powershell -File packaging/make-icons.ps1
Add-Type -AssemblyName System.Drawing
$out = Join-Path $PSScriptRoot "..\assets\icon"
New-Item -ItemType Directory -Force $out | Out-Null

function Render([int]$size, [double]$inset, [string]$path) {
  $bmp = New-Object Drawing.Bitmap $size, $size
  $g = [Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.Clear([Drawing.Color]::Transparent)

  $pad = $size * $inset
  $w = $size - 2 * $pad
  $r = $w * 0.225
  $rect = New-Object Drawing.RectangleF $pad, $pad, $w, $w
  $path2 = New-Object Drawing.Drawing2D.GraphicsPath
  $d = 2 * $r
  $path2.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
  $path2.AddArc($rect.Right - $d, $rect.Y, $d, $d, 270, 90)
  $path2.AddArc($rect.Right - $d, $rect.Bottom - $d, $d, $d, 0, 90)
  $path2.AddArc($rect.X, $rect.Bottom - $d, $d, $d, 90, 90)
  $path2.CloseFigure()

  # Accent gradient: #1a8cff -> #005fb8 (the app's light-mode accent).
  $brush = New-Object Drawing.Drawing2D.LinearGradientBrush $rect, `
    ([Drawing.Color]::FromArgb(255, 0x1a, 0x8c, 0xff)), ([Drawing.Color]::FromArgb(255, 0x00, 0x5f, 0xb8)), 90
  $g.FillPath($brush, $path2)

  # The logo path "M8 5l-5 7 5 7M16 5l5 7-5 7" on a 24-unit grid, centred in the tile.
  $unit = $w / 24 * 0.78
  $ox = $pad + ($w - 24 * $unit) / 2
  $oy = $pad + ($w - 24 * $unit) / 2
  function P($x, $y) { New-Object Drawing.PointF ($ox + $x * $unit), ($oy + $y * $unit) }
  $pen = New-Object Drawing.Pen ([Drawing.Color]::White), ([single]([Math]::Max(1.2, 2.6 * $unit)))
  $pen.StartCap = [Drawing.Drawing2D.LineCap]::Round
  $pen.EndCap = [Drawing.Drawing2D.LineCap]::Round
  $pen.LineJoin = [Drawing.Drawing2D.LineJoin]::Round
  $g.DrawLines($pen, [Drawing.PointF[]]@((P 8 5), (P 3 12), (P 8 19)))
  $g.DrawLines($pen, [Drawing.PointF[]]@((P 16 5), (P 21 12), (P 16 19)))

  $bmp.Save($path, [Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}

# Windows / Linux: near full-bleed tiles.
foreach ($s in 16, 24, 32, 48, 64, 128, 256, 512) { Render $s 0.04 (Join-Path $out "icon-$s.png") }
# macOS: Big Sur grid (824px body inside a 1024px canvas).
Render 1024 0.0977 (Join-Path $out "icon-macos-1024.png")

# Pack icon.ico with PNG-compressed entries (supported since Windows Vista).
$sizes = 16, 24, 32, 48, 64, 128, 256
$images = $sizes | ForEach-Object { ,[IO.File]::ReadAllBytes((Join-Path $out "icon-$_.png")) }
$ms = New-Object IO.MemoryStream
$bw = New-Object IO.BinaryWriter $ms
$bw.Write([UInt16]0); $bw.Write([UInt16]1); $bw.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
  $s = $sizes[$i]; $len = $images[$i].Length
  $bw.Write([byte]($(if ($s -ge 256) { 0 } else { $s }))); $bw.Write([byte]($(if ($s -ge 256) { 0 } else { $s })))
  $bw.Write([byte]0); $bw.Write([byte]0); $bw.Write([UInt16]1); $bw.Write([UInt16]32)
  $bw.Write([UInt32]$len); $bw.Write([UInt32]$offset); $offset += $len
}
foreach ($img in $images) { $bw.Write($img) }
[IO.File]::WriteAllBytes((Join-Path $out "icon.ico"), $ms.ToArray())
"icons written to $out"
