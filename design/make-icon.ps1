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

# ---- 背景：深色对角渐变圆角方块 ----
$bgPath = New-RoundedRectPath 32 32 960 960 220
$bgBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(0, 0)),
    (New-Object System.Drawing.Point($size, $size)),
    [System.Drawing.Color]::FromArgb(255, 30, 34, 46),
    [System.Drawing.Color]::FromArgb(255, 16, 18, 26))
$g.FillPath($bgBrush, $bgPath)

# ---- 背景网格点阵（harness/终端感）----
$dotBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(26, 255, 255, 255))
for ($px = 120; $px -lt 920; $px += 56) {
    for ($py = 110; $py -lt 920; $py += 56) {
        $g.FillEllipse($dotBrush, $px, $py, 6, 6)
    }
}

# ---- 背景顶部发光弧（微妙的科技感）----
$glowPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(40, 94, 234, 212), 3)
$glowPath = New-RoundedRectPath 32 32 960 960 220
$g.DrawPath($glowPen, $glowPath)

# ---- 中央徽章：蓝紫渐变圆角矩形 ----
$badgeX = 172; $badgeY = 292; $badgeW = 680; $badgeH = 440; $badgeR = 96
$badgePath = New-RoundedRectPath $badgeX $badgeY $badgeW $badgeH $badgeR
$badgeBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(172, 292)),
    (New-Object System.Drawing.Point(852, 732)),
    [System.Drawing.Color]::FromArgb(255, 79, 124, 255),
    [System.Drawing.Color]::FromArgb(255, 139, 92, 246))
$g.FillPath($badgeBrush, $badgePath)

# 徽章内高光（顶部半透明白）
$hlBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(172, 292)),
    (New-Object System.Drawing.Point(172, 520)),
    [System.Drawing.Color]::FromArgb(60, 255, 255, 255),
    [System.Drawing.Color]::FromArgb(0, 255, 255, 255))
$hlPath = New-RoundedRectPath 172 292 680 230 96
$g.FillPath($hlBrush, $hlPath)

# ---- "DSH" 粗体文字（居中）----
$font = New-Object System.Drawing.Font("Segoe UI", 300, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
$format = New-Object System.Drawing.StringFormat
$format.Alignment = [System.Drawing.StringAlignment]::Center
$format.LineAlignment = [System.Drawing.StringAlignment]::Center
$rect = New-Object System.Drawing.RectangleF($badgeX, $badgeY, $badgeW, $badgeH)
$white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
$g.DrawString("DSH", $font, $white, $rect, $format)

# ---- 徽章右下角终端光标（模拟 DSH_ 输入状态）----
$cursorBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 94, 234, 212))
$g.FillRectangle($cursorBrush, 640, 636, 26, 96)

# ---- 底部状态灯（青色小圆点，AI 在线感）----
$lightBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 94, 234, 212))
$g.FillEllipse($lightBrush, 496, 790, 32, 32)

$outDir = "E:\1\15\dsh-desktop\design"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$out = Join-Path $outDir "icon-1024.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Host "saved: $out"

$g.Dispose(); $bmp.Dispose()
