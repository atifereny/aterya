//! The dock's native shell: glass window, AppBar reservation, taskbar hiding, fullscreen hide.
pub mod appbar;
pub mod autohide;
pub mod fullscreen;
pub mod glass;
pub mod taskbar;

use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU8, Ordering};
use tauri::{Emitter, Manager, WebviewWindow};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTOPRIMARY};
use windows_core::BOOL;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{ABN_FULLSCREENAPP, ABN_POSCHANGED};
use windows::Win32::UI::WindowsAndMessaging::{
    RegisterWindowMessageW, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA,
    WM_DISPLAYCHANGE, WM_DPICHANGED,
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
/// The screen edge the dock sits on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

static EDGE: AtomicU8 = AtomicU8::new(0);

fn edge_code(name: &str) -> u8 {
    match name {
        "top" => 1,
        "left" => 2,
        "right" => 3,
        _ => 0,
    }
}

pub(crate) fn edge() -> Edge {
    match EDGE.load(Ordering::Relaxed) {
        1 => Edge::Top,
        2 => Edge::Left,
        3 => Edge::Right,
        _ => Edge::Bottom,
    }
}

pub(crate) fn edge_top() -> bool {
    edge() == Edge::Top
}

/// A dock on the left or right edge runs top to bottom.
pub(crate) fn vertical() -> bool {
    matches!(edge(), Edge::Left | Edge::Right)
}

/// The dock's strip `thick` px thick along its edge of `mon`.
pub(crate) fn edge_rect(mon: RECT, thick: i32) -> RECT {
    match edge() {
        Edge::Bottom => RECT { top: mon.bottom - thick, ..mon },
        Edge::Top => RECT { bottom: mon.top + thick, ..mon },
        Edge::Left => RECT { right: mon.left + thick, ..mon },
        Edge::Right => RECT { left: mon.right - thick, ..mon },
    }
}

/// Where an icon sits along the dock (physical px): `c` is its center in the dock page's CSS px along the
/// dock's own axis (x for a horizontal dock, y for a vertical one).
pub(crate) fn along_pos(dock: RECT, c: f64) -> i32 {
    (if vertical() { dock.top } else { dock.left }) + (c * dock_k()).round() as i32
}

/// Top-left of a `w` x `h` popup opening `gap` px clear of the dock window `dock`, on the side facing the
/// screen's middle, centered on `along` (physical px along the dock) and kept `margin` px inside `mon`.
pub(crate) fn popup_pos(dock: RECT, along: i32, (w, h): (i32, i32), gap: i32, margin: i32, mon: RECT) -> (i32, i32) {
    match edge() {
        Edge::Bottom | Edge::Top => {
            let x = (along - w / 2).min(mon.right - w - margin).max(mon.left + margin);
            let y = if edge_top() { (dock.bottom + gap).min(mon.bottom - h) } else { (dock.top - gap - h).max(mon.top) };
            (x, y)
        }
        side => {
            let y = (along - h / 2).min(mon.bottom - h - margin).max(mon.top + margin);
            let x = if side == Edge::Left { (dock.right + gap).min(mon.right - w) } else { (dock.left - gap - w).max(mon.left) };
            (x, y)
        }
    }
}

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
    /// Screen number (1-based): the user's order, see `Config::monitor_order`.
    pub num: u32,
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> BOOL {
    let v = unsafe { &mut *(out.0 as *mut Vec<MonitorInfo>) };
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(h, &mut mi as *mut _ as *mut MONITORINFO) }.as_bool() {
        let r = mi.monitorInfo.rcMonitor;
        let n = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(mi.szDevice.len());
        v.push(MonitorInfo { device: String::from_utf16_lossy(&mi.szDevice[..n]), x: r.left, y: r.top, width: r.right - r.left, height: r.bottom - r.top, primary: mi.monitorInfo.dwFlags & 1 != 0, num: 0 });
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

/// Device names in screen-number order: the user's list (connected screens only), then the rest left to right.
pub(crate) fn numbered() -> Vec<String> {
    let cfg = crate::config::Config::load(&crate::config::config_path());
    let all = monitors();
    let mut out: Vec<String> = Vec::new();
    for d in cfg.monitor_order.iter().chain(all.iter().map(|m| &m.device)) {
        if all.iter().any(|m| &m.device == d) && !out.contains(d) {
            out.push(d.clone());
        }
    }
    out
}

/// Connected screens left to right, each with its screen number.
#[tauri::command]
pub fn monitors_list() -> Vec<MonitorInfo> {
    let order = numbered();
    let mut v = monitors();
    for m in &mut v {
        m.num = order.iter().position(|d| *d == m.device).map_or(0, |i| i as u32 + 1);
    }
    v
}

/// Saves the screen numbering: `order` lists the devices, first is screen 1.
#[tauri::command]
pub fn set_monitor_order(app: tauri::AppHandle, order: Vec<String>) -> Result<(), String> {
    let mut c = crate::settings::current()?;
    c.monitor_order = order;
    crate::settings::save_and_emit(&app, &c)
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
    let want = if auto { edge_rect(mon, height) } else { appbar::query(hwnd, mon, height) };
    let mut last = PLACED.lock().unwrap_or_else(|e| e.into_inner());
    if needs_place(*last, want, dpi) {
        let rc = if auto { want } else { appbar::set_pos(hwnd, want) };
        *last = Some((want, dpi));
        log::info!("[shell] dock placed at {},{} {}x{}", rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top);
        let hidden = auto && HIDDEN.load(Ordering::Relaxed);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let (x, y) = match (hidden, edge()) {
            (false, _) => (rc.left, rc.top),
            (true, Edge::Bottom) => (rc.left, mon.bottom),
            (true, Edge::Top) => (rc.left, mon.top - h),
            (true, Edge::Left) => (mon.left - w, rc.top),
            (true, Edge::Right) => (mon.right, rc.top),
        };
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
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
    let edge_changed = EDGE.swap(edge_code(&cfg.dock_edge), Ordering::Relaxed) != edge_code(&cfg.dock_edge);
    if !cfg.dock_autohide {
        HIDDEN.store(false, Ordering::Relaxed);
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Ok(win) = main_window(&app2) else { return };
        let _ = win.set_zoom(z as f64);
        if let Ok(hwnd) = win.hwnd() {
            if (cfg.dock_autohide && !was_auto) || edge_changed {
                appbar::unregister(hwnd); // give the work area back (a new edge registers afresh)
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
    shutdown_early();
    taskbar::spawn_guardian(); // first, so a crash from here on still gets the taskbar back
    taskbar::restore_all(); // undo a previous run that died with no guardian
    taskbar::hide_all();
    {
        let cfg = crate::config::Config::load(&crate::config::config_path());
        *CHOSEN.lock().unwrap_or_else(|e| e.into_inner()) = cfg.dock_monitor.clone();
        GAP.store((crate::settings::dock_gap(&cfg) as f32).to_bits(), Ordering::Relaxed);
        ZOOM.store((crate::settings::dock_scale(&cfg) as f32).to_bits(), Ordering::Relaxed);
        AUTOHIDE.store(cfg.dock_autohide, Ordering::Relaxed);
        EDGE.store(edge_code(&cfg.dock_edge), Ordering::Relaxed);
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

/// Put the dock back on top and re-evaluate the fullscreen state. Used after a Windows flyout closes,
/// which can leave the dock hidden or under the shell's topmost windows.
pub(crate) fn reassert() {
    let h = APPBAR_HWND.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    fullscreen::check();
    if is_visible() {
        unsafe {
            let _ = SetWindowPos(HWND(h as *mut _), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
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

/// `teardown` for the end of a Windows session. Explorer may already be gone or stuck, and the calls
/// into it block; Windows then waits on this process and lists it as "preventing shutdown". Give it a second at most.
fn teardown_bounded() {
    let t = std::thread::spawn(teardown);
    let end = std::time::Instant::now() + std::time::Duration::from_millis(1000);
    while !t.is_finished() && std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The WebView2 processes of this app: the browser process it started and everything below that (renderer, gpu,
/// utility). Not other children of this process, such as the new version an update has just started.
fn webview_processes() -> Vec<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    let mut all: Vec<(u32, u32, String)> = Vec::new(); // (pid, parent, exe name)
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return vec![] };
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            all.push((e.th32ProcessID, e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..n]).to_lowercase()));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    let me = std::process::id();
    let mut out: Vec<u32> = all.iter().filter(|(_, par, name)| *par == me && name == "msedgewebview2.exe").map(|(p, _, _)| *p).collect();
    let mut i = 0;
    while i < out.len() {
        let parent = out[i];
        let found: Vec<u32> = all.iter().filter(|(p, par, _)| *par == parent && !out.contains(p)).map(|(p, _, _)| *p).collect();
        out.extend(found);
        i += 1;
    }
    out
}

/// Stop this app's WebView2 processes. Left to themselves they notice the host's exit late, and Windows then lists
/// them (and this app) as "preventing shutdown".
fn stop_webviews() -> usize {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let kids = webview_processes();
    for pid in &kids {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, *pid) {
                let _ = TerminateProcess(h, 0);
                let _ = CloseHandle(h);
            }
        }
    }
    kids.len()
}

/// The Windows session is ending (this window got WM_ENDSESSION first): give the taskbar back and stop WebView2.
pub(crate) fn end_session() {
    log::info!("[shell] session ending");
    teardown_bounded();
    let n = stop_webviews();
    log::info!("[shell] session ending: stopped {n} WebView2 process(es)");
    log::logger().flush();
}

unsafe extern "C" {
    fn atexit(f: extern "C" fn()) -> i32;
}

/// At every exit of the process, whichever window or thread causes it (tao answers WM_ENDSESSION with
/// `process::exit` from its own hidden window, bypassing everything else).
extern "C" fn at_exit() {
    stop_webviews();
}

/// Be among the first to be closed at logoff, and never make Windows wait on a retry dialog for this process.
pub(crate) fn shutdown_early() {
    use windows::Win32::System::Threading::SetProcessShutdownParameters;
    const SHUTDOWN_NORETRY: u32 = 1;
    let _ = unsafe { SetProcessShutdownParameters(0x4FF, SHUTDOWN_NORETRY) };
    unsafe { atexit(at_exit) };
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

/// Pill bounds in CSS px from the frontend; the pill sits at the dock window's screen-side edge.
/// Returns whether DWM blur is live, so the frontend can fall back to a solid pill.
#[tauri::command]
pub fn set_pill_bounds(window: WebviewWindow, x: f64, y: f64, width: f64, height: f64) -> Result<bool, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    // Composition objects belong to the main thread.
    let fill = crate::settings::current().map_or(0.3, |c| c.theme.opacity as f32);
    window
        .run_on_main_thread(move || {
            glass::set_clear(fill);
            glass::set_region(HWND(hwnd as *mut _), x, y, width, height, radius_css())
        })
        .map_err(|e| e.to_string())?;
    Ok(glass::blur_active())
}

/// Fill 0 hides the blur layer. Called when the theme changes.
pub fn set_clear(app: &tauri::AppHandle, fill: f32) {
    let _ = app.run_on_main_thread(move || glass::set_clear(fill));
}
