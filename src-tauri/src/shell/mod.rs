//! The dock's native shell: glass window, AppBar reservation.
pub mod appbar;
pub mod glass;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicIsize, Ordering};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use windows::Win32::Foundation::{HWND, RECT};

/// Payload: whether DWM blur is live (false: paint the solid pill).
pub const GLASS_EVENT: &str = "dock://glass";

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
/// Dock HWND while the AppBar is registered; 0 once removed. Cached because "main" is already
/// gone from the window manager by the time RunEvent::Exit fires.
static APPBAR_HWND: AtomicIsize = AtomicIsize::new(0);

fn blur_changed(blur: bool) {
    if let Some(app) = APP.get() {
        let _ = app.emit(GLASS_EVENT, blur);
    }
}

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
    let _ = APP.set(app.clone());
    let rc = appbar::register(hwnd, monitor, height);
    APPBAR_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    win.set_size(PhysicalSize::new((rc.right - rc.left) as u32, (rc.bottom - rc.top) as u32))?;
    win.set_position(PhysicalPosition::new(rc.left, rc.top))?;
    glass::apply(hwnd, blur_changed); // after sizing, so tao's restyle on resize can't bring the caption back
    win.show()
}

/// Give the work area back. Idempotent: safe from both ExitRequested and Exit.
pub fn teardown() {
    let h = APPBAR_HWND.swap(0, Ordering::SeqCst);
    if h != 0 {
        appbar::unregister(HWND(h as *mut _));
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
