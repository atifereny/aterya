//! The dock's native shell: glass window, AppBar reservation, taskbar hiding, fullscreen hide.
pub mod appbar;
pub mod autohide;
pub mod fullscreen;
pub mod glass;
pub mod taskbar;

use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use tauri::{Emitter, Manager, WebviewWindow};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTOPRIMARY};
use windows_core::BOOL;
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
/// Auto-hide on, and whether the dock is currently slid below the screen edge.
pub(crate) static AUTOHIDE: AtomicBool = AtomicBool::new(false);
pub(crate) static HIDDEN: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_visible() -> bool {
    VISIBLE.load(Ordering::Relaxed)
}

fn blur_changed(blur: bool) {
    if let Some(app) = APP.get() {
        let _ = app.emit(GLASS_EVENT, blur);
    }
}

const DOCK_CSS: f64 = 64.0;
/// Pill corner radius in CSS px (f32 bits); set from the config.
static RADIUS: AtomicU32 = AtomicU32::new(0x41b0_0000); // 22.0

fn radius_css() -> f64 {
    f32::from_bits(RADIUS.load(Ordering::Relaxed)) as f64
}

/// The dock page's zoom (f32 bits) and its gap to the screen edge (CSS px, f32 bits).
static ZOOM: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0
static GAP: AtomicU32 = AtomicU32::new(0x4080_0000); // 4.0
/// Device name of the chosen screen; empty means the primary one.
static CHOSEN: Mutex<String> = Mutex::new(String::new());

pub(crate) fn zoom() -> f64 {
    f32::from_bits(ZOOM.load(Ordering::Relaxed)) as f64
}

/// Physical px per CSS px of the dock page (DPI and the dock's own zoom).
pub(crate) fn dock_k() -> f64 {
    unsafe { GetDpiForWindow(dock_hwnd()) }.max(96) as f64 / 96.0 * zoom()
}

fn main_window(app: &tauri::AppHandle) -> tauri::Result<WebviewWindow> {
    app.get_webview_window("main").ok_or(tauri::Error::WindowNotFound)
}

/// One connected screen: Windows device name, physical rect, whether it is the primary one.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub device: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> BOOL {
    let v = unsafe { &mut *(out.0 as *mut Vec<MonitorInfo>) };
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(h, &mut mi as *mut _ as *mut MONITORINFO) }.as_bool() {
        let r = mi.monitorInfo.rcMonitor;
        let n = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(mi.szDevice.len());
        v.push(MonitorInfo { device: String::from_utf16_lossy(&mi.szDevice[..n]), x: r.left, y: r.top, width: r.right - r.left, height: r.bottom - r.top, primary: mi.monitorInfo.dwFlags & 1 != 0 });
    }
    BOOL(1)
}

/// Connected screens, left to right.
pub(crate) fn monitors() -> Vec<MonitorInfo> {
    let mut v: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut v as *mut _ as isize));
    }
    v.sort_by_key(|m| (m.x, m.y));
    v
}

#[tauri::command]
pub fn monitors_list() -> Vec<MonitorInfo> {
    monitors()
}

/// The screen the dock sits on: the chosen one if it is connected, else the primary screen.
pub(crate) fn primary_monitor() -> RECT {
    let chosen = CHOSEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if !chosen.is_empty() {
        if let Some(m) = monitors().into_iter().find(|m| m.device == chosen) {
            return RECT { left: m.x, top: m.y, right: m.x + m.width, bottom: m.y + m.height };
        }
    }
    let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe {
        let _ = GetMonitorInfoW(MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY), &mut mi);
    }
    mi.rcMonitor
}

/// Last reservation (granted rect, DPI); None forces the next place() to re-reserve.
static PLACED: Mutex<Option<(RECT, u32)>> = Mutex::new(None);
static PLACING: AtomicBool = AtomicBool::new(false);

/// Whether a reservation needs SETPOS + a move: only when the rect or DPI differs from the last one.
fn needs_place(last: Option<(RECT, u32)>, rc: RECT, dpi: u32) -> bool {
    last != Some((rc, dpi))
}

/// (Re)claim the primary monitor's bottom edge at the current DPI and move the dock there.
/// Re-run whenever the shell, DPI or display layout changes. Unchanged results do nothing, so
/// ABN_POSCHANGED from our own or another appbar's SETPOS cannot ping-pong.
fn place(hwnd: HWND) {
    if PLACING.swap(true, Ordering::SeqCst) {
        return; // SETPOS / SetWindowPos can re-enter through the subclass
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let gap = f32::from_bits(GAP.load(Ordering::Relaxed)) as f64;
    let height = ((DOCK_CSS * zoom() + gap) * dpi as f64 / 96.0).round() as i32;
    let mon = primary_monitor();
    let auto = AUTOHIDE.load(Ordering::Relaxed);
    // Auto-hide keeps no work area: the dock floats over the windows.
    let want = if auto { RECT { top: mon.bottom - height, ..mon } } else { appbar::query(hwnd, mon, height) };
    let mut last = PLACED.lock().unwrap_or_else(|e| e.into_inner());
    if needs_place(*last, want, dpi) {
        let rc = if auto { want } else { appbar::set_pos(hwnd, want) };
        *last = Some((want, dpi));
        log::info!("[shell] dock placed at {},{} {}x{}", rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top);
        let y = if auto && HIDDEN.load(Ordering::Relaxed) { mon.bottom } else { rc.top };
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), rc.left, y, rc.right - rc.left, rc.bottom - rc.top, SWP_NOACTIVATE);
        }
    }
    drop(last);
    PLACING.store(false, Ordering::SeqCst);
}

/// Read screen, size and gap from the config and re-place the dock (also used at startup and on a change).
pub fn apply_dock(app: &tauri::AppHandle) {
    let cfg = crate::config::Config::load(&crate::config::config_path());
    *CHOSEN.lock().unwrap_or_else(|e| e.into_inner()) = cfg.dock_monitor.clone();
    let z = crate::settings::dock_scale(&cfg) as f32;
    GAP.store((crate::settings::dock_gap(&cfg) as f32).to_bits(), Ordering::Relaxed);
    ZOOM.store(z.to_bits(), Ordering::Relaxed);
    RADIUS.store((crate::settings::dock_radius(&cfg) as f32).to_bits(), Ordering::Relaxed);
    let was_auto = AUTOHIDE.swap(cfg.dock_autohide, Ordering::Relaxed);
    if !cfg.dock_autohide {
        HIDDEN.store(false, Ordering::Relaxed);
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Ok(win) = main_window(&app2) else { return };
        let _ = win.set_zoom(z as f64);
        if let Ok(hwnd) = win.hwnd() {
            if cfg.dock_autohide && !was_auto {
                appbar::unregister(hwnd); // give the work area back
            }
            *PLACED.lock().unwrap_or_else(|e| e.into_inner()) = None;
            place(hwnd);
        }
        let _ = win.eval("window.dispatchEvent(new Event('resize'))"); // the pill re-reports its bounds
    });
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
    {
        let cfg = crate::config::Config::load(&crate::config::config_path());
        *CHOSEN.lock().unwrap_or_else(|e| e.into_inner()) = cfg.dock_monitor.clone();
        GAP.store((crate::settings::dock_gap(&cfg) as f32).to_bits(), Ordering::Relaxed);
        ZOOM.store((crate::settings::dock_scale(&cfg) as f32).to_bits(), Ordering::Relaxed);
        AUTOHIDE.store(cfg.dock_autohide, Ordering::Relaxed);
        RADIUS.store((crate::settings::dock_radius(&cfg) as f32).to_bits(), Ordering::Relaxed);
        let _ = win.set_zoom(zoom());
    }
    place(hwnd);
    APPBAR_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    glass::apply(hwnd, blur_changed); // after sizing, so tao's restyle on resize can't bring the caption back
    fullscreen::start();
    autohide::start();
    win.show()
}

/// Restart only the dock: reload its page, hide the Windows taskbar again and re-claim the screen edge.
/// The rest of the app (settings, tray, floating widgets) keeps running.
pub fn restart_dock(app: &tauri::AppHandle) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Ok(win) = main_window(&app2) else { return };
        if let Ok(hwnd) = win.hwnd() {
            taskbar::hide_all();
            *PLACED.lock().unwrap_or_else(|e| e.into_inner()) = None;
            place(hwnd);
        }
        let _ = win.eval("location.reload()");
        log::info!("[shell] dock restarted");
    });
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
            *PLACED.lock().unwrap_or_else(|e| e.into_inner()) = None; // the new explorer forgot our reservation
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

#[cfg(test)]
mod tests {
    use super::needs_place;
    use windows::Win32::Foundation::RECT;

    const RC: RECT = RECT { left: 0, top: 1366, right: 2560, bottom: 1440 };

    #[test]
    fn place_only_on_change() {
        assert!(needs_place(None, RC, 96));
        assert!(!needs_place(Some((RC, 96)), RC, 96));
        assert!(needs_place(Some((RC, 96)), RC, 144));
        assert!(needs_place(Some((RECT { top: 1318, ..RC }, 96)), RC, 96));
    }
}

/// Pill bounds in CSS px from the frontend; the pill sits at the top of the window.
/// Returns whether DWM blur is live, so the frontend can fall back to a solid pill.
#[tauri::command]
pub fn set_pill_bounds(window: WebviewWindow, x: f64, width: f64, height: f64) -> Result<bool, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    // Composition objects belong to the main thread.
    let fill = crate::settings::current().map_or(0.3, |c| c.theme.opacity as f32);
    window
        .run_on_main_thread(move || {
            glass::set_clear(fill);
            glass::set_region(HWND(hwnd as *mut _), x, width, height, radius_css())
        })
        .map_err(|e| e.to_string())?;
    Ok(glass::blur_active())
}

/// Fill 0 hides the blur layer. Called when the theme changes.
pub fn set_clear(app: &tauri::AppHandle, fill: f32) {
    let _ = app.run_on_main_thread(move || glass::set_clear(fill));
}
