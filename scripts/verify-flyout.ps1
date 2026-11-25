# Cold-start check of the hidden-icons button on a given Aterya exe: starts it fresh, clicks the chevron once,
# follows the foreground window for 3.5 s and reads dock.log for a wrongly hidden dock.
# Usage: powershell -NoProfile -File scripts\verify-flyout.ps1 -Exe <path to exe> [-X 1809 -Y 1405]
# X, Y: the chevron on screen (default is the 2560x1440 layout of the author). The app is restarted by this test.
param([Parameter(Mandatory)][string]$Exe, [int]$X = 1809, [int]$Y = 1405)

Add-Type @"
using System;using System.Runtime.InteropServices;using System.Text;
public class FlyoutCheck{
  [DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")]public static extern int GetClassName(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out R r);
  [DllImport("user32.dll")]public static extern IntPtr GetAncestor(IntPtr h,uint f);
  [DllImport("user32.dll")]public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")]public static extern void mouse_event(uint f,int x,int y,uint d,UIntPtr e);
  public struct R{public int l,t,r,b;}
  public static string Info(){IntPtr h=GetAncestor(GetForegroundWindow(),2);var s=new StringBuilder(128);GetClassName(h,s,128);R r;GetWindowRect(h,out r);return s+" "+r.l+","+r.t+","+r.r+","+r.b;}
}
"@

Get-Process | Where-Object { $_.Name -like 'Aterya*' } | Stop-Process -Force
Start-Sleep 3
Start-Process $Exe
Start-Sleep 10

$log = Join-Path $env:APPDATA 'custom_taskbar\logs\dock.log'
$before = (Get-Content $log | Measure-Object).Count
[FlyoutCheck]::SetCursorPos($X, $Y); Start-Sleep -Milliseconds 400
[FlyoutCheck]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 40; [FlyoutCheck]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)

$prev = ''; $sw = [Diagnostics.Stopwatch]::StartNew()
while ($sw.ElapsedMilliseconds -lt 3500) {
  $i = [FlyoutCheck]::Info()
  if ($i -ne $prev) { '{0,5} ms  {1}' -f $sw.ElapsedMilliseconds, $i; $prev = $i }
  Start-Sleep -Milliseconds 15
}
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait('{ESC}')
Start-Sleep -Milliseconds 800

$lines = Get-Content $log | Select-Object -Skip $before | Where-Object { $_ -match 'fullscreen\]|dock (shown|hidden)' }
'--- dock.log (fullscreen / hidden lines since the click):'
$lines | ForEach-Object { $_.Substring(0, [Math]::Min(170, $_.Length)) }
if ($lines | Where-Object { $_ -match 'hiding for' }) { 'RESULT: FAIL (dock was hidden for a non-fullscreen window)'; exit 2 } else { 'RESULT: OK' }
