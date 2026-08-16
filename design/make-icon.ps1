Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$g.Clear([System.Drawing.Color]::Transparent)

function New-RoundedRectPath($x, $y, $w, $h, $r) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $path.AddArc($x, $y, $d, $d, 180, 90)
    $path.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $path.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $path.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

$pt0 = New-Object System.Drawing.Point(0, 0)
$pt1 = New-Object System.Drawing.Point($size, $size)
$pt2 = New-Object System.Drawing.Point(16, 16)
$pt3 = New-Object System.Drawing.Point(520, 520)
$cTop = [System.Drawing.Color]::FromArgb(255, 77, 107, 254)
$cBot = [System.Drawing.Color]::FromArgb(255, 124, 58, 237)
$cHl1 = [System.Drawing.Color]::FromArgb(70, 255, 255, 255)
$cHl2 = [System.Drawing.Color]::FromArgb(0, 255, 255, 255)

# ---- 背景：明亮蓝紫对角渐变圆角方块 ----
$bgPath = New-RoundedRectPath 16 16 992 992 210
$bgBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush($pt0, $pt1, $cTop, $cBot)
$g.FillPath($bgBrush, $bgPath)

# 右上角高光
$hlBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush($pt2, $pt3, $cHl1, $cHl2)
$hlPath = New-RoundedRectPath 16 16 992 992 210
$g.FillPath($hlBrush, $hlPath)

# ---- 中央终端符号 ">_" 用 GraphicsPath 精确控制 ----
$textPath = New-Object System.Drawing.Drawing2D.GraphicsPath
$family = New-Object System.Drawing.FontFamily("Consolas")
$emSize = 430.0
$origin = New-Object System.Drawing.PointF(0, 0)
$fmt = New-Object System.Drawing.StringFormat
$fmt.Alignment = [System.Drawing.StringAlignment]::Center
$fmt.LineAlignment = [System.Drawing.StringAlignment]::Center
$textPath.AddString(">_", $family, [int][System.Drawing.FontStyle]::Bold, $emSize, $origin, $fmt)

# 把路径平移到画布中央
$bounds = $textPath.GetBounds()
$offsetX = ($size - $bounds.Width) / 2 - $bounds.X
$offsetY = ($size - $bounds.Height) / 2 - $bounds.Y
$translate = New-Object System.Drawing.Drawing2D.Matrix
$translate.Translate($offsetX, $offsetY)
$textPath.Transform($translate)

$white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$g.FillPath($white, $textPath)

# ---- 右下角青色状态灯 ----
$cGlow = [System.Drawing.Color]::FromArgb(50, 94, 234, 212)
$cLight = [System.Drawing.Color]::FromArgb(255, 94, 234, 212)
$glowBrush = New-Object System.Drawing.SolidBrush($cGlow)
$g.FillEllipse($glowBrush, 796, 796, 96, 96)
$lightBrush = New-Object System.Drawing.SolidBrush($cLight)
$g.FillEllipse($lightBrush, 812, 812, 64, 64)

$outDir = "E:\1\15\dsh-desktop\design"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$out = Join-Path $outDir "icon-1024.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Host "saved: $out"

$g.Dispose(); $bmp.Dispose()
