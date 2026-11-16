//! Reserve the bottom screen edge so maximized windows stop above the dock.
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Shell::{SHAppBarMessage, ABE_BOTTOM, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

fn data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, hWnd: hwnd, uCallbackMessage: WM_APP + 1, uEdge: ABE_BOTTOM, ..Default::default() }
}

/// Register as a bottom appbar `height_px` (physical) tall on the monitor `monitor` (physical rect).
/// Returns the rect the shell granted; move the window there.
// shortcut: ABN_POSCHANGED/fullscreen callbacks are not handled (no window subclass); add a
// subclass when other appbars or resolution changes need to move the dock.
pub fn register(hwnd: HWND, monitor: RECT, height_px: i32) -> RECT {
    let mut d = data(hwnd);
    unsafe {
        SHAppBarMessage(ABM_NEW, &mut d);
        d.rc = RECT { top: monitor.bottom - height_px, ..monitor };
        SHAppBarMessage(ABM_QUERYPOS, &mut d);
        d.rc.top = d.rc.bottom - height_px; // shell may lift the bottom above another appbar
        SHAppBarMessage(ABM_SETPOS, &mut d);
    }
    d.rc
}

pub fn unregister(hwnd: HWND) {
    let mut d = data(hwnd);
    unsafe { SHAppBarMessage(ABM_REMOVE, &mut d) };
    eprintln!("[appbar] ABM_REMOVE sent, work area released");
}
