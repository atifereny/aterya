# Emergency recovery: bring the Windows taskbar back if the dock left it hidden.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\restore-taskbar.ps1
# Kills any running dock/guardian, shows every taskbar window, turns auto-hide off,
# and deletes the dock's saved taskbar state so the next start records a fresh original.
$ErrorActionPreference = 'Continue'

Get-Process custom_taskbar -ErrorAction SilentlyContinue | Stop-Process -Force

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TaskbarRestore {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct APPBARDATA {
        public int cbSize; public IntPtr hWnd; public uint uCallbackMessage; public uint uEdge;
        public RECT rc; public IntPtr lParam;
    }
    [DllImport("shell32.dll")] public static extern UIntPtr SHAppBarMessage(uint msg, ref APPBARDATA d);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);

    public static int Run() {
        int shown = 0;
        IntPtr tray = FindWindowExW(IntPtr.Zero, IntPtr.Zero, "Shell_TrayWnd", null);
        if (tray != IntPtr.Zero) {
            var d = new APPBARDATA();
            d.cbSize = Marshal.SizeOf(d); d.hWnd = tray; d.lParam = (IntPtr)2; // ABS_ALWAYSONTOP: auto-hide off
            SHAppBarMessage(10, ref d); // ABM_SETSTATE
            ShowWindow(tray, 5); shown++; // SW_SHOW
        }
        IntPtr h = IntPtr.Zero;
        while ((h = FindWindowExW(IntPtr.Zero, h, "Shell_SecondaryTrayWnd", null)) != IntPtr.Zero) { ShowWindow(h, 5); shown++; }
        return shown;
    }
}
'@

$n = [TaskbarRestore]::Run()
Remove-Item (Join-Path $env:APPDATA 'custom_taskbar\taskbar-state.json') -ErrorAction SilentlyContinue
if ($n -eq 0) { Write-Output 'No taskbar window found: is explorer.exe running? Start it with: start explorer'; exit 1 }
Write-Output "Taskbar restored ($n window(s) shown, auto-hide off)."
