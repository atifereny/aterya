//! The dock's native shell: glass window, AppBar reservation, taskbar hiding, fullscreen hide.
pub mod appbar;
pub mod fullscreen;
pub mod glass;
pub mod taskbar;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use tauri::{Emitter, Manager, WebviewWindow};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{ABN_FULLSCREENAPP, ABN_POSCHANGED};
use windows::Win32::UI::WindowsAndMessaging::{
    RegisterWindowMessageW, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNA,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENDSESSION,
};

/// Payload: whether DWM blur is live (false: paint the solid pill).
pub const GLASS_EVENT: &str = "dock://glass";
/// Payload: whether the dock is shown (false while a fullscreen app owns the primary monitor).
pub const VISIBILITY_EVENT: &str = "dock://visibility";

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
/// Dock HWND while the AppBar is registered; 0 once removed. Cached because "main" is already
/// gone from the window manager by the time RunEvent::Exit fires.
static APPBAR_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static VISIBLE: AtomicBool = AtomicBool::new(true);

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

pub(crate) fn primary_monitor() -> RECT {
    let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe {
        let _ = GetMonitorInfoW(MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY), &mut mi);
    }
    mi.rcMonitor
}

/// (Re)claim the primary monitor's bottom edge at the current DPI and move the dock there.
/// Re-run whenever the shell, DPI or display layout changes (ABM_NEW is a no-op when registered).
fn place(hwnd: HWND) {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let height = ((DOCK_CSS + MARGIN_CSS) * dpi as f64 / 96.0).round() as i32;
    let rc = appbar::register(hwnd, primary_monitor(), height);
    log::info!("[shell] dock placed at {},{} {}x{}", rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top);
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top, SWP_NOACTIVATE);
    }
}

/// Size the dock across the primary monitor's bottom edge, hide the Windows taskbar, reserve, glass, show.
pub fn setup(app: &tauri::AppHandle) -> tauri::Result<()> {
    let win = main_window(app)?;
    let hwnd = win.hwnd()?;
    let _ = APP.set(app.clone());
    TASKBAR_CREATED.store(unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }, Ordering::SeqCst);
    taskbar::spawn_guardian(); // first, so a crash from here on still gets the taskbar back
    taskbar::restore_all(); // undo a previous run that died with no guardian
    taskbar::hide_all();
    place(hwnd);
    APPBAR_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    glass::apply(hwnd, blur_changed); // after sizing, so tao's restyle on resize can't bring the caption back
    fullscreen::start();
    win.show()
}

/// Window messages the glass subclass passes on (after default handling).
pub(crate) fn on_message(hwnd: HWND, msg: u32, wp: WPARAM, _lp: LPARAM) {
    match msg {
        m if m == appbar::CALLBACK && wp.0 as u32 == ABN_POSCHANGED => place(hwnd),
        m if m == appbar::CALLBACK && wp.0 as u32 == ABN_FULLSCREENAPP => fullscreen::check(),
        WM_DPICHANGED | WM_DISPLAYCHANGE => place(hwnd),
        WM_ENDSESSION if wp.0 != 0 => teardown(),
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::SeqCst) => {
            log::info!("[shell] TaskbarCreated: explorer restarted, hiding again");
            taskbar::hide_all();
            place(hwnd);
        }
        _ => {}
    }
}

/// Show or hide the dock (fullscreen apps) and tell the frontend. No-op if unchanged.
pub(crate) fn set_visible(visible: bool) {
    let h = APPBAR_HWND.load(Ordering::SeqCst);
    if h == 0 || VISIBLE.swap(visible, Ordering::SeqCst) == visible {
        return;
    }
    unsafe {
        let _ = ShowWindow(HWND(h as *mut _), if visible { SW_SHOWNA } else { SW_HIDE });
    }
    log::info!("[shell] dock {}", if visible { "shown" } else { "hidden for fullscreen app" });
    if let Some(app) = APP.get() {
        let _ = app.emit(VISIBILITY_EVENT, visible);
    }
}

pub(crate) fn dock_hwnd() -> HWND {
    HWND(APPBAR_HWND.load(Ordering::SeqCst) as *mut _)
}

/// Give the work area and the Windows taskbar back. Idempotent: safe from ExitRequested, Exit,
/// WM_ENDSESSION and the panic hook.
pub fn teardown() {
    let h = APPBAR_HWND.swap(0, Ordering::SeqCst);
    if h != 0 {
        appbar::unregister(HWND(h as *mut _));
    }
    taskbar::restore_all();
}

/// Pill bounds in CSS px from the frontend; the pill sits at the top of the window.
/// Returns whether DWM blur is live, so the frontend can fall back to a solid pill.
#[tauri::command]
pub fn set_pill_bounds(window: WebviewWindow, x: f64, width: f64, height: f64) -> Result<bool, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    glass::set_region(hwnd, x, width, height, PILL_RADIUS_CSS);
    Ok(glass::blur_active())
}
