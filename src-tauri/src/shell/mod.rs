//! The dock's native shell: glass window, AppBar reservation.
pub mod appbar;
pub mod glass;

use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use windows::Win32::Foundation::RECT;

const DOCK_CSS: f64 = 64.0;
const MARGIN_CSS: f64 = 10.0;
const PILL_RADIUS_CSS: f64 = 22.0;

fn main_window(app: &tauri::AppHandle) -> tauri::Result<WebviewWindow> {
    app.get_webview_window("main").ok_or(tauri::Error::WindowNotFound)
}

/// Size the dock across the primary monitor's bottom edge, reserve it, apply glass, show.
// shortcut: sized once at startup; DPI/resolution changes need a restart until a WM_DPICHANGED handler exists.
pub fn setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    let win = main_window(app)?;
    let hwnd = win.hwnd()?;
    let mon = win.primary_monitor()?.ok_or(tauri::Error::WindowNotFound)?;
    let (pos, size) = (mon.position(), mon.size());
    let height = ((DOCK_CSS + MARGIN_CSS) * mon.scale_factor()).round() as i32;
    let monitor = RECT { left: pos.x, top: pos.y, right: pos.x + size.width as i32, bottom: pos.y + size.height as i32 };
    let rc = appbar::register(hwnd, monitor, height);
    win.set_size(PhysicalSize::new((rc.right - rc.left) as u32, (rc.bottom - rc.top) as u32))?;
    win.set_position(PhysicalPosition::new(rc.left, rc.top))?;
    glass::apply(hwnd); // after sizing, so tao's restyle on resize can't bring the caption back
    win.show()
}

pub fn teardown(app: &tauri::AppHandle) {
    if let Ok(hwnd) = main_window(app).and_then(|w| w.hwnd()) {
        appbar::unregister(hwnd);
    }
}

/// Pill bounds in CSS px from the frontend; the pill sits at the top of the window.
/// Returns whether DWM blur is live, so the frontend can fall back to a solid pill.
#[tauri::command]
pub fn set_pill_bounds(window: WebviewWindow, x: f64, width: f64, height: f64) -> Result<bool, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    glass::set_region(hwnd, x, width, height, PILL_RADIUS_CSS);
    Ok(glass::blur_active())
}
