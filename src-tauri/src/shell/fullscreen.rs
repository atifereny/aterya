//! Hide the dock while a fullscreen app (video, game, presentation) owns the primary monitor.
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MonitorFromWindow, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::Shell::SHQueryUserNotificationState;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetForegroundWindow, GetShellWindow, GetWindowRect, IsIconic, IsZoomed, EVENT_SYSTEM_FOREGROUND,
    GA_ROOT, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

/// Foreground windows that are the shell itself, never a fullscreen app (desktop, taskbars, Task View / Alt+Tab).
const SHELL_CLASSES: [&str; 8] = [
    "Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd", "XamlExplorerHostIslandWindow",
    "TopLevelWindowForOverflowXamlIsland", "Xaml_WindowedPopupClass", // the hidden-icons flyout and its popups
    "Windows.UI.Core.CoreWindow", // Start, Search, widgets: shell hosts that can span the whole monitor (see `fullscreen_state`)
];

/// What Windows says about the user's state (SHQueryUserNotificationState) as a fullscreen verdict. A window that
/// merely covers the monitor is not enough: shell hosts and maximized windows do too. Windows reports BUSY (2) or
/// APP (7) only when something really is fullscreen, and ACCEPTS_NOTIFICATIONS (5) otherwise.
pub fn fullscreen_state(covers: bool, quns: i32) -> bool {
    match quns {
        3 | 4 => true,      // D3D exclusive, presentation mode: true even when the window rect is odd
        2 | 7 => covers,    // busy, or a fullscreen store app: it must be the window on this monitor
        _ => false,
    }
}

/// True when `win` covers all of `mon` (borderless games often overhang by a few px).
pub fn covers_monitor(win: RECT, mon: RECT) -> bool {
    win.left <= mon.left && win.top <= mon.top && win.right >= mon.right && win.bottom >= mon.bottom
}

fn is_fullscreen(fg: HWND) -> bool {
    if fg.is_invalid() || fg == unsafe { GetShellWindow() } || fg == super::dock_hwnd() || unsafe { IsIconic(fg) }.as_bool() {
        return false;
    }
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(fg, &mut buf) } as usize;
    if SHELL_CLASSES.contains(&String::from_utf16_lossy(&buf[..n]).as_str()) {
        return false;
    }
    // Fullscreen apps on other monitors leave the primary-monitor dock alone (QUNS is system-wide).
    let primary = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
    if unsafe { MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) } != primary {
        return false;
    }
    // D3D exclusive / presentation mode report true even when the window rect is odd.
    let quns = unsafe { SHQueryUserNotificationState() }.unwrap_or_default();
    let mut rc = RECT::default();
    let got = unsafe { GetWindowRect(fg, &mut rc) }.is_ok();
    let mon = super::primary_monitor();
    // A normally maximized window overhangs its screen by the invisible frame; with auto-hide (no reserved
    // strip) it covers the whole screen without being fullscreen. Real fullscreen fits the screen exactly.
    let exact = rc.left == mon.left && rc.top == mon.top && rc.right == mon.right && rc.bottom == mon.bottom;
    let framed_max = unsafe { IsZoomed(fg) }.as_bool() && !exact;
    let covers = got && covers_monitor(rc, mon) && !framed_max;
    let full = fullscreen_state(covers, quns.0);
    if full && super::is_visible() {
        log::info!("[fullscreen] hiding for class={} rect={},{},{},{} quns={}", String::from_utf16_lossy(&buf[..n]), rc.left, rc.top, rc.right, rc.bottom, quns.0);
    }
    full
}

/// Re-evaluate the foreground window: hide the dock over fullscreen apps, show it otherwise.
pub fn check() {
    let fg = unsafe { GetForegroundWindow() };
    let fg = if fg.is_invalid() { fg } else { unsafe { GetAncestor(fg, GA_ROOT) } };
    super::set_visible(!is_fullscreen(fg));
}

unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    check();
}

/// Watch foreground changes (runs on the calling thread's message loop: call from the main thread).
/// Fullscreen toggles inside the same window (F11, a video's fullscreen button) arrive as ABN_FULLSCREENAPP.
pub fn start() {
    let hook = unsafe {
        SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_foreground), 0, 0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS)
    };
    if hook.is_invalid() {
        log::error!("[fullscreen] SetWinEventHook failed; dock will stay visible over fullscreen apps");
    }
    // A missed foreground event, or Windows updating its notification state a moment late, heals within a second.
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        check();
    });
}

#[cfg(test)]
mod tests {
    use super::covers_monitor;
    use windows::Win32::Foundation::RECT;

    const MON: RECT = RECT { left: 0, top: 0, right: 2560, bottom: 1440 };

    #[test]
    fn notification_state_decides() {
        use super::fullscreen_state as f;
        assert!(!f(true, 5), "a shell host covering the monitor while Windows accepts notifications");
        assert!(!f(true, 6));
        assert!(f(true, 2), "a fullscreen video or borderless game");
        assert!(!f(false, 2), "busy because of a window on another monitor");
        assert!(f(false, 3) && f(false, 4), "d3d exclusive and presentation mode");
    }

    #[test]
    fn equal_rects_cover() {
        assert!(covers_monitor(MON, MON));
    }

    #[test]
    fn borderless_overhang_covers() {
        assert!(covers_monitor(RECT { left: -8, top: -8, right: 2568, bottom: 1448 }, MON));
    }

    #[test]
    fn maximized_above_appbar_does_not_cover() {
        assert!(!covers_monitor(RECT { left: -8, top: -8, right: 2568, bottom: 1366 }, MON));
    }

    #[test]
    fn window_on_other_monitor_does_not_cover() {
        assert!(!covers_monitor(RECT { left: 0, top: 1440, right: 2560, bottom: 2160 }, MON));
    }
}
