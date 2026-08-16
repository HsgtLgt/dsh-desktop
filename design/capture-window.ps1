param(
    [string]$TitlePattern,
    [string]$OutPath
)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
[Win32]::SetProcessDPIAware() | Out-Null

# enumerate windows by title (FindWindow needs exact title; use EnumWindows to match pattern)
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class WinEnum {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);
    public static List<IntPtr> FindByTitle(string pattern) {
        var result = new List<IntPtr>();
        EnumWindows((h, l) => {
            if (IsWindowVisible(h)) {
                var sb = new StringBuilder(256);
                GetWindowText(h, sb, 256);
                if (sb.ToString().IndexOf(pattern, StringComparison.OrdinalIgnoreCase) >= 0) {
                    result.Add(h);
                }
            }
            return true;
        }, IntPtr.Zero);
        return result;
    }
}
"@

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$handles = [WinEnum]::FindByTitle($TitlePattern)
if ($handles.Count -eq 0) { Write-Host "NO WINDOW FOUND for '$TitlePattern'"; exit 1 }
$h = $handles[0]
[Win32]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400

$rect = New-Object Win32+RECT
[Win32]::GetWindowRect($h, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$ht = $rect.Bottom - $rect.Top
Write-Host "window '$TitlePattern' rect: $($rect.Left),$($rect.Top) ${w}x${ht}"

$bmp = New-Object System.Drawing.Bitmap($w, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bmp.Size)
$bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Host "saved: $OutPath"
$g.Dispose(); $bmp.Dispose()
