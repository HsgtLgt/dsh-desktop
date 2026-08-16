$csharp = @"
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Text;

public static class IconGen
{
    public static GraphicsPath RoundedRect(float x, float y, float w, float h, float r)
    {
        var path = new GraphicsPath();
        float d = r * 2;
        path.AddArc(x, y, d, d, 180, 90);
        path.AddArc(x + w - d, y, d, d, 270, 90);
        path.AddArc(x + w - d, y + h - d, d, d, 0, 90);
        path.AddArc(x, y + h - d, d, d, 90, 90);
        path.CloseFigure();
        return path;
    }

    public static void Generate(string outPath)
    {
        const int size = 1024;
        using (var bmp = new Bitmap(size, size))
        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            g.Clear(Color.Transparent);

            // ---- background: bright blue-purple diagonal gradient ----
            var bgPath = RoundedRect(16, 16, 992, 992, 210);
            using (var bgBrush = new LinearGradientBrush(
                new Point(0, 0), new Point(size, size),
                Color.FromArgb(255, 77, 107, 254),
                Color.FromArgb(255, 124, 58, 237)))
            {
                g.FillPath(bgBrush, bgPath);
            }

            // ---- top-right highlight ----
            using (var hlBrush = new LinearGradientBrush(
                new Point(16, 16), new Point(520, 520),
                Color.FromArgb(80, 255, 255, 255),
                Color.FromArgb(0, 255, 255, 255)))
            {
                g.FillPath(hlBrush, bgPath);
            }

            // ---- terminal symbol ">_" centered via GraphicsPath ----
            using (var textPath = new GraphicsPath())
            using (var family = new FontFamily("Consolas"))
            {
                var fmt = new StringFormat
                {
                    Alignment = StringAlignment.Center,
                    LineAlignment = StringAlignment.Center
                };
                textPath.AddString(">_", family, (int)FontStyle.Bold, 430f,
                    new PointF(0, 0), fmt);

                var bounds = textPath.GetBounds();
                float dx = (size - bounds.Width) / 2 - bounds.X;
                float dy = (size - bounds.Height) / 2 - bounds.Y;
                using (var m = new Matrix())
                {
                    m.Translate(dx, dy);
                    textPath.Transform(m);
                }

                using (var white = new SolidBrush(Color.FromArgb(255, 255, 255, 255)))
                {
                    g.FillPath(white, textPath);
                }
            }

            // ---- bottom-right cyan status dot ----
            using (var glow = new SolidBrush(Color.FromArgb(50, 94, 234, 212)))
            {
                g.FillEllipse(glow, 796, 796, 96, 96);
            }
            using (var light = new SolidBrush(Color.FromArgb(255, 94, 234, 212)))
            {
                g.FillEllipse(light, 812, 812, 64, 64);
            }

            bmp.Save(outPath, System.Drawing.Imaging.ImageFormat.Png);
        }
    }
}
"@

Add-Type -TypeDefinition $csharp -ReferencedAssemblies System.Drawing
[IconGen]::Generate("E:\1\15\dsh-desktop\design\icon-1024.png")
Write-Host "saved"
