//! Reserve the bottom screen edge so maximized windows stop above the dock.
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Shell::{SHAppBarMessage, ABE_BOTTOM, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

/// Message the shell sends the dock for ABN_* notifications (wParam = the ABN code).
pub const CALLBACK: u32 = WM_APP + 1;

fn data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, hWnd: hwnd, uCallbackMessage: CALLBACK, uEdge: ABE_BOTTOM, ..Default::default() }
}

/// Register (no-op if already registered) as a bottom appbar `height_px` (physical) tall on
/// `monitor` (physical rect) and return the rect the shell would grant. Sends no notifications.
pub fn query(hwnd: HWND, monitor: RECT, height_px: i32) -> RECT {
    let mut d = data(hwnd);
    unsafe {
        SHAppBarMessage(ABM_NEW, &mut d);
        d.rc = RECT { top: monitor.bottom - height_px, ..monitor };
        SHAppBarMessage(ABM_QUERYPOS, &mut d);
    }
    d.rc.top = d.rc.bottom - height_px; // shell may lift the bottom above another appbar
    d.rc
}

/// Reserve `rc` (from `query`). Other appbars get ABN_POSCHANGED, so only call it on a change.
pub fn set_pos(hwnd: HWND, rc: RECT) -> RECT {
    let mut d = APPBARDATA { rc, ..data(hwnd) };
    unsafe { SHAppBarMessage(ABM_SETPOS, &mut d) };
    d.rc
}

pub fn unregister(hwnd: HWND) {
    let mut d = data(hwnd);
    unsafe { SHAppBarMessage(ABM_REMOVE, &mut d) };
    log::info!("[appbar] ABM_REMOVE sent, work area released");
}
