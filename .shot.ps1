param([string]$Out=".shot.png")
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Drawing.Imaging;
public class W {
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, R2, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int hi, uint f);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr c);
}
'@ -ReferencedAssemblies System.Drawing,System.Windows.Forms
[W]::SetProcessDpiAwarenessContext([IntPtr]::new(-4)) | Out-Null
$p = Get-Process python | Where-Object { $_.MainWindowTitle -eq 'WinScan' } | Select-Object -First 1
if (-not $p) { Write-Host 'no WinScan'; exit }
$h = $p.MainWindowHandle
[W]::ShowWindow($h, 9) | Out-Null
[W]::SetWindowPos($h, [IntPtr]::new(-1), 0, 0, 0, 0, 0x43) | Out-Null
Start-Sleep -Milliseconds 900
$r = New-Object W+R
[W]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.R2 - $r.L; $h2 = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $h2
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object System.Drawing.Size $w, $h2))
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
[W]::SetWindowPos($h, [IntPtr]::new(-2), 0, 0, 0, 0, 0x43) | Out-Null
Write-Host "saved $Out ($w x $h2)"
