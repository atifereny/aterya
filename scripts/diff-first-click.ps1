# Starts an Aterya exe fresh, clicks once and reports every moment the dock strip changed by more than a clock
# tick would: a glitch after the first click shows as a burst of changed pixels.
# Usage: powershell -NoProfile -File scripts\diff-first-click.ps1 -Exe <exe> [-ClickX 900 -ClickY 500 -Wait 12]
param([Parameter(Mandatory)][string]$Exe, [int]$ClickX = 900, [int]$ClickY = 500, [int]$Wait = 12, [int]$StripTop = 1370, [int]$StripHeight = 70)

Add-Type -ReferencedAssemblies System.Drawing @"
using System;using System.Runtime.InteropServices;using System.Drawing;using System.Drawing.Imaging;
public class FD{
  [DllImport("user32.dll")]public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")]public static extern void mouse_event(uint f,int x,int y,uint d,UIntPtr e);
  public static int Diff(Bitmap a,Bitmap b){
    var r=new Rectangle(0,0,a.Width,a.Height);
    var da=a.LockBits(r,ImageLockMode.ReadOnly,PixelFormat.Format32bppArgb); var db=b.LockBits(r,ImageLockMode.ReadOnly,PixelFormat.Format32bppArgb);
    int bytes=Math.Abs(da.Stride)*a.Height; var x=new byte[bytes]; var y=new byte[bytes];
    Marshal.Copy(da.Scan0,x,0,bytes); Marshal.Copy(db.Scan0,y,0,bytes);
    a.UnlockBits(da); b.UnlockBits(db);
    int n=0; for(int i=0;i<bytes;i+=4){ if(Math.Abs(x[i]-y[i])+Math.Abs(x[i+1]-y[i+1])+Math.Abs(x[i+2]-y[i+2])>60) n++; }
    return n;
  }
}
"@
Add-Type -AssemblyName System.Drawing

Get-Process | Where-Object { $_.Name -like 'Aterya*' } | Stop-Process -Force
Start-Sleep 3
Start-Process $Exe
Start-Sleep $Wait

function Grab { $b = New-Object Drawing.Bitmap 2560, $StripHeight; $g = [Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0, $StripTop, 0, 0, $b.Size); $g.Dispose(); $b }
[FD]::SetCursorPos($ClickX, $ClickY); Start-Sleep -Milliseconds 300
$frames = @(); $ts = @(); $sw = [Diagnostics.Stopwatch]::StartNew(); $clickAt = -1
for ($i = 0; $i -lt 110; $i++) {
  if ($i -eq 15) { [FD]::mouse_event(2,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 30; [FD]::mouse_event(4,0,0,0,[UIntPtr]::Zero); $clickAt = $sw.ElapsedMilliseconds }
  $frames += ,(Grab); $ts += $sw.ElapsedMilliseconds; Start-Sleep -Milliseconds 25
}
"click at $clickAt ms, $($frames.Count) frames over $($sw.ElapsedMilliseconds) ms"
$spikes = 0
for ($i = 1; $i -lt $frames.Count; $i++) {
  $d = [FD]::Diff($frames[$i-1], $frames[$i])
  if ($d -gt 400) { $spikes++; '{0,5} ms  frame {1}: {2} px changed' -f $ts[$i], $i, $d }
}
if ($spikes -eq 0) { 'no visual burst in the dock strip' }
