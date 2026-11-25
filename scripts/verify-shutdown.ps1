# Simulates the end of a Windows session on a running Aterya the way Windows does it (WM_QUERYENDSESSION, then
# WM_ENDSESSION to every top-level window in turn) and reports how fast the app and its WebView2 children go away.
# Usage: powershell -File scripts\verify-shutdown.ps1 [-Name Aterya]
# The app is closed by this test; start it again afterwards.
param([string]$Name = 'Aterya', [switch]$Reverse)

Add-Type @"
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Text;
public class EndSession{
  public delegate bool EP(IntPtr h,IntPtr l);
  [DllImport("user32.dll")]public static extern bool EnumWindows(EP f,IntPtr l);
  [DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
  [DllImport("user32.dll")]public static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll")]public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr r);
  [DllImport("user32.dll")]public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
  public static List<KeyValuePair<long,string>> Wins(uint pid){
    var l=new List<KeyValuePair<long,string>>();
    EnumWindows((h,x)=>{uint p;GetWindowThreadProcessId(h,out p);if(p==pid){var t=new StringBuilder(128);GetWindowText(h,t,128);l.Add(new KeyValuePair<long,string>(h.ToInt64(),t.ToString()));}return true;},IntPtr.Zero);
    return l;
  }
}
"@

$procs = Get-Process -Name "$Name*" -ErrorAction SilentlyContinue
$main = $procs | Where-Object { [EndSession]::Wins([uint32]$_.Id).Count -gt 0 } | Select-Object -First 1
if (-not $main) { Write-Output "FAIL: no running $Name with windows"; exit 1 }
$wins = [EndSession]::Wins([uint32]$main.Id)
# Windows visits the windows in no fixed order; the dock window first and last are the two orders that matter.
$order = @($wins | Sort-Object { $_.Value -ne 'Aterya' })
if ($Reverse) { $order = @($wins | Sort-Object { $_.Value -eq 'Aterya' }) }

$slow = 0
foreach ($w in $wins) {
  $r = [IntPtr]::Zero; $sw = [Diagnostics.Stopwatch]::StartNew()
  $ok = [EndSession]::SendMessageTimeout([IntPtr]$w.Key, 0x11, [IntPtr]::Zero, [IntPtr]::Zero, 2, 3000, [ref]$r)
  if ($sw.ElapsedMilliseconds -gt 300 -or $ok -eq [IntPtr]::Zero) { $slow++; Write-Output "slow QUERYENDSESSION answer: $($w.Key) $($w.Value) $($sw.ElapsedMilliseconds) ms" }
}
Write-Output "QUERYENDSESSION: $($wins.Count) windows, $slow slow"

$log = Join-Path $env:APPDATA 'custom_taskbar\logs\dock.log'
$before = (Get-Content $log | Measure-Object).Count
$kids = { (Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object { $_.CommandLine -match 'customtaskbar' } | Measure-Object).Count }
$kidsBefore = & $kids
$sw = [Diagnostics.Stopwatch]::StartNew()
foreach ($w in $order) {
  if (-not (Get-Process -Id $main.Id -ErrorAction SilentlyContinue)) { break }
  $r = [IntPtr]::Zero
  [void][EndSession]::SendMessageTimeout([IntPtr]$w.Key, 0x16, [IntPtr]1, [IntPtr]::Zero, 2, 2000, [ref]$r)
}
while ((Get-Process -Id $main.Id -ErrorAction SilentlyContinue) -and $sw.ElapsedMilliseconds -lt 8000) { Start-Sleep -Milliseconds 20 }
$goneMs = $sw.ElapsedMilliseconds
$alive = [bool](Get-Process -Id $main.Id -ErrorAction SilentlyContinue)
Start-Sleep -Milliseconds 700
$kidsAfter = & $kids
Write-Output "process gone after $goneMs ms (still alive: $alive); WebView2 children $kidsBefore -> $kidsAfter"
Get-Content $log | Select-Object -Skip $before | Where-Object { $_ -notmatch 'glass\]' } | ForEach-Object { Write-Output ("log: " + $_.Substring(0, [Math]::Min(150, $_.Length))) }
if ($alive -or $kidsAfter -gt 0 -or $slow -gt 0) { Write-Output "RESULT: FAIL"; exit 2 } else { Write-Output "RESULT: OK" }
