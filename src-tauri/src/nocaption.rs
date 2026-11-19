//! Popup and floating windows must never show a title bar. tao puts WS_CAPTION back on undecorated windows
//! (on focus changes and when shown), which flashed a title and a white bar before the page painted.
//! This blocks those style bits for good.
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{
            GWL_STYLE, GetWindowLongPtrW, STYLESTRUCT, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            SetWindowLongPtrW, SetWindowPos, WM_NCACTIVATE, WM_STYLECHANGING, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU,
        },
    },
};

const BITS: u32 = WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0;

unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
    match msg {
        WM_STYLECHANGING if wp.0 as i32 == GWL_STYLE.0 => {
            let ss = unsafe { &mut *(lp.0 as *mut STYLESTRUCT) };
            ss.styleNew &= !BITS;
        }
        // No inactive frame painted over the page.
        WM_NCACTIVATE => return unsafe { DefSubclassProc(hwnd, msg, wp, LPARAM(-1)) },
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// Main thread only.
pub fn apply(hwnd: isize) {
    let h = HWND(hwnd as *mut _);
    unsafe {
        let _ = SetWindowSubclass(h, Some(subclass), 77, 0);
        SetWindowLongPtrW(h, GWL_STYLE, GetWindowLongPtrW(h, GWL_STYLE) & !(BITS as isize));
        let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}
