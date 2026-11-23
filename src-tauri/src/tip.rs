//! Name tooltip above a dock icon: a small click-through popup that never takes the focus. The page measures its
//! own text and reports the size back, then the window is placed centered above the icon.
use std::sync::atomic::{AtomicI32, Ordering};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};

const ABOVE_DOCK_CSS: i32 = 6;
static ICON_CX: AtomicI32 = AtomicI32::new(0); // physical px, screen coordinates

fn ensure(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("tip") {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, "tip", WebviewUrl::App("index.html#tip".into()))
        .title("")
        .additional_browser_args(crate::BROWSER_ARGS)
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focusable(false)
        .visible(false)
        .inner_size(160.0, 32.0)
        .build()
        .map_err(|e| e.to_string())?;
    let h = w.hwnd().map_err(|e| e.to_string())?.0 as isize;
    w.run_on_main_thread(move || {
        crate::nocaption::apply(h);
        unsafe {
            let hwnd = HWND(h as *mut _);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TRANSPARENT.0 | WS_EX_TOOLWINDOW.0) as isize);
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(w)
}

/// Built hidden ahead of the first hover, so the first tooltip does not flash an unpainted window.
pub fn warm(app: &AppHandle) {
    let _ = ensure(app);
}

/// Pointer rests on a dock icon: `icon_cx` is its center x in the dock's CSS px.
#[tauri::command]
pub async fn tip_show(app: AppHandle, text: String, sub: String, icon: String, icon_cx: f64) -> Result<(), String> {
    ensure(&app)?;
    let dock = crate::shell::dock_hwnd();
    let mut dr = RECT::default();
    unsafe { GetWindowRect(dock, &mut dr) }.map_err(|e| e.to_string())?;
    ICON_CX.store(dr.left + (icon_cx * crate::shell::dock_k()).round() as i32, Ordering::SeqCst);
    app.emit_to("tip", "tip://show", serde_json::json!({ "text": text, "sub": sub, "icon": icon })).map_err(|e| e.to_string())
}

/// The page measured itself (CSS px): place the window above the icon and show it.
#[tauri::command]
pub async fn tip_ready(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let a = app.clone();
    app.run_on_main_thread(move || {
        let Some(Ok(me)) = a.get_webview_window("tip").map(|w| w.hwnd()) else { return };
        let dock = crate::shell::dock_hwnd();
        let k = unsafe { GetDpiForWindow(dock) }.max(96) as f64 / 96.0;
        let (w, h) = ((width * k).ceil() as i32, (height * k).ceil() as i32);
        let mut dr = RECT::default();
        if unsafe { GetWindowRect(dock, &mut dr) }.is_err() {
            return;
        }
        let mon = crate::shell::primary_monitor();
        let x = (ICON_CX.load(Ordering::SeqCst) - w / 2).min(mon.right - w - 4).max(mon.left + 4);
        let y = crate::shell::popup_y(dr, h, (ABOVE_DOCK_CSS as f64 * k) as i32, mon);
        unsafe {
            let _ = SetWindowPos(me, Some(HWND_TOPMOST), x, y, w, h, SWP_SHOWWINDOW | SWP_NOACTIVATE);
        }
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tip_hide(app: AppHandle) -> Result<(), String> {
    let a = app.clone();
    app.run_on_main_thread(move || {
        if let Some(Ok(h)) = a.get_webview_window("tip").map(|w| w.hwnd()) {
            unsafe {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
    })
    .map_err(|e| e.to_string())
}
