# Starts an Aterya exe fresh, waits, clicks once on the screen and prints every change of the Windows taskbar,
# the dock window and the foreground window for a few seconds: a taskbar that shows up, a dock that moves or hides.
# Usage: powershell -NoProfile -File scripts\watch-first-click.ps1 -Exe <exe> [-ClickX 900 -ClickY 500 -Wait 12]
param([string]$Exe = '', [string]$ProcName = 'Aterya*', [int]$ClickX = 900, [int]$ClickY = 500, [int]$Wait = 12, [switch]$NoRestart)

Add-Type @"
using System;using System.Runtime.InteropServices;using System.Text;
public class W{
  [DllImport("user32.dll")]public static extern IntPtr FindWindow(string c,string t);
  [DllImport("user32.dll")]public static extern IntPtr FindWindowEx(IntPtr p,IntPtr a,string c,string t);
  [DllImport("user32.dll")]public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out R r);
  [DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")]public static extern int GetClassName(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll")]public static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
  [DllImport("user32.dll")]public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")]public static extern void mouse_event(uint f,int x,int y,uint d,UIntPtr e);
  public delegate bool EP(IntPtr h,IntPtr l);
  [DllImport("user32.dll")]public static extern bool EnumWindows(EP f,IntPtr l);
  public struct R{public int l,t,r,b;}
  public static string Rect(IntPtr h){ if(h==IntPtr.Zero) return "none"; R r; GetWindowRect(h,out r); return IsWindowVisible(h)+" "+r.l+","+r.t+","+r.r+","+r.b; }
  public static string Fg(){ var h=GetForegroundWindow(); var c=new StringBuilder(128); GetClassName(h,c,128); R r; GetWindowRect(h,out r); return c+" "+r.l+","+r.t+","+r.r+","+r.b; }
  public static IntPtr Dock(uint pid){ IntPtr found=IntPtr.Zero; EnumWindows((h,x)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p==pid){ var t=new StringBuilder(64); GetWindowText(h,t,64); if(t.ToString()=="Aterya"){found=h;return false;} } return true; },IntPtr.Zero); return found; }
}
"@

if (-not $NoRestart) {
  Get-Process | Where-Object { $_.Name -like $ProcName } | Stop-Process -Force
  Start-Sleep 3
  Start-Process $Exe
  Start-Sleep $Wait
}
$p = Get-Process | Where-Object { $_.Name -like $ProcName -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$pidMain = if ($p) { [uint32]$p.Id } else { 0 }
if ($pidMain -eq 0) { $pidMain = [uint32](Get-Process | Where-Object { $_.Name -like $ProcName } | Sort-Object StartTime | Select-Object -Last 1).Id }
$dock = [W]::Dock($pidMain)
$tray = [W]::FindWindow('Shell_TrayWnd', $null)
$sec = [W]::FindWindow('Shell_SecondaryTrayWnd', $null)
"dock hwnd: $dock; Shell_TrayWnd: $tray"

$state = @{}
$sw = [Diagnostics.Stopwatch]::StartNew(); $clicked = $false
while ($sw.ElapsedMilliseconds -lt 4000) {
  if (-not $clicked -and $sw.ElapsedMilliseconds -ge 1000) {
    [W]::SetCursorPos($ClickX, $ClickY); Start-Sleep -Milliseconds 30
    [W]::mouse_event(2,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 40; [W]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
    $clicked = $true; '{0,5} ms  --- click ---' -f $sw.ElapsedMilliseconds
  }
  $now = @{ tray = [W]::Rect($tray); secondary = [W]::Rect($sec); dock = [W]::Rect($dock); foreground = [W]::Fg() }
  foreach ($k in $now.Keys) { if ($state[$k] -ne $now[$k]) { '{0,5} ms  {1}: {2}' -f $sw.ElapsedMilliseconds, $k, $now[$k]; $state[$k] = $now[$k] } }
  Start-Sleep -Milliseconds 4
}
