//! Windows-style hover preview: a second, non-activating window above the dock listing a group's
//! windows. Live thumbnails are DWM thumbnails drawn over the window (they sit above the webview,
//! so the page keeps titles/close buttons in a header row and leaves the thumbnail slot empty).
//! Hover timing (dwell, grace) lives here so it works across the dock and preview webviews.
use super::tracker;
use serde::Serialize;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Dwm::{
        DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_VISIBLE, DwmQueryThumbnailSourceSize,
        DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
use windows_core::BOOL;

const DWELL_MS: u64 = 500;
const SWITCH_MS: u64 = 120; // already showing: moving to another icon swaps quickly
const GRACE_MS: u64 = 250;
const THUMB: (i32, i32) = (200, 120);
const TITLE_H: i32 = 22;
const PAD: i32 = 10;
const GAP: i32 = 8;
const PER_ROW: usize = 5;
// shortcut: more than 10 windows in one group are not listed, upgrade with scrolling when someone has that many.
const MAX_WINDOWS: usize = 10;
const ABOVE_DOCK_CSS: i32 = 8;

#[derive(Debug, PartialEq)]
struct Layout {
    w: i32,
    h: i32,
    cells: Vec<(i32, i32)>,
}

/// Popup size and cell (title row + thumbnail slot) top-left corners, CSS px.
fn layout(n: usize) -> Layout {
    let (cw, ch) = (THUMB.0, TITLE_H + THUMB.1);
    let cols = n.clamp(1, PER_ROW) as i32;
    let rows = n.div_ceil(PER_ROW).max(1) as i32;
    let cells = (0..n as i32)
        .map(|i| (PAD + (i % PER_ROW as i32) * (cw + GAP), PAD + (i / PER_ROW as i32) * (ch + GAP)))
        .collect();
    Layout { w: 2 * PAD + cols * cw + (cols - 1) * GAP, h: 2 * PAD + rows * ch + (rows - 1) * GAP, cells }
}

/// Largest size with the source's aspect ratio that fits in `max`.
fn fit(src: (i32, i32), max: (i32, i32)) -> (i32, i32) {
    if src.0 <= 0 || src.1 <= 0 {
        return max;
    }
    let s = (max.0 as f64 / src.0 as f64).min(max.1 as f64 / src.1 as f64);
    (((src.0 as f64 * s).round() as i32).max(1), ((src.1 as f64 * s).round() as i32).max(1))
}

/// Top-left of a popup of `size` centered on `icon_cx`, bottom edge at `bottom`, kept inside `mon`.
fn place(size: (i32, i32), icon_cx: i32, bottom: i32, mon: RECT) -> (i32, i32) {
    let x = (icon_cx - size.0 / 2).min(mon.right - size.0).max(mon.left);
    (x, (bottom - size.1).max(mon.top))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    hwnd: String, // string: handles can exceed JS safe integers
    title: String,
    minimized: bool,
    x: i32,
    y: i32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    items: Vec<Item>,
    icon: String,
    thumb_w: i32,
    thumb_h: i32,
    title_h: i32,
}

struct Pv {
    shown: bool,
    group: String,
    icon_cx: f64,
    windows: Vec<isize>,
    thumbs: Vec<isize>,
    payload: Option<Payload>,
}

static PV: Mutex<Pv> =
    Mutex::new(Pv { shown: false, group: String::new(), icon_cx: 0.0, windows: Vec::new(), thumbs: Vec::new(), payload: None });
/// Bumped by every hover event; a timer acts only if nothing happened since it was armed.
static GEN: AtomicU64 = AtomicU64::new(0);

/// Whether the window preview popup is showing.
pub fn shown() -> bool {
    PV.lock().map(|p| p.shown).unwrap_or(false)
}

/// (feature on, modifier) read from the config each time the popup is shown.
static BRING: Mutex<(bool, String)> = Mutex::new((false, String::new()));

/// While the popup is up, pressing the configured modifier sends the hovered app's windows to the
/// primary monitor. One thread for the process lifetime; it idles unless the popup is shown.
fn spawn_key_watch(app: AppHandle) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::thread::spawn(move || {
            let mut was_held = false;
            loop {
                std::thread::sleep(Duration::from_millis(25));
                let (on, key) = BRING.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let held = on && pv().shown && !matches!(key.as_str(), "" | "always") && super::pins::key_held(&key);
                if held && !was_held {
                    let wins = pv().windows.clone();
                    for &w in &wins {
                        super::pins::move_to_main(w);
                    }
                    if let Some(&first) = wins.first() {
                        tracker::activate(first);
                    }
                    hide(&app);
                }
                was_held = held;
            }
        });
    });
}

fn pv() -> std::sync::MutexGuard<'static, Pv> {
    PV.lock().unwrap_or_else(|e| e.into_inner())
}

fn after(ms: u64, g: u64, f: impl FnOnce() + Send + 'static) {
    // shortcut: one short-lived thread per hover event, upgrade to a single timer thread if hover traffic ever shows up in profiles.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if GEN.load(Ordering::SeqCst) == g {
            f();
        }
    });
}

unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCACTIVATE => unsafe { DefSubclassProc(hwnd, msg, wp, LPARAM(-1)) }, // no inactive frame painted over the page
        _ => unsafe { DefSubclassProc(hwnd, msg, wp, lp) },
    }
}

fn ensure(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("preview") {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, "preview", WebviewUrl::App("index.html#preview".into()))
        .title("")
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .focused(false)
        .focusable(false)
        .inner_size(220.0, 160.0)
        .build()
        .map_err(|e| e.to_string())?;
    let h = w.hwnd().map_err(|e| e.to_string())?.0 as isize;
    w.run_on_main_thread(move || unsafe {
        let hwnd = HWND(h as *mut _);
        let _ = SetWindowSubclass(hwnd, Some(subclass), 1, 0);
        crate::nocaption::apply(h);
    })
    .map_err(|e| e.to_string())?;
    Ok(w)
}

fn unregister(pv: &mut Pv) {
    for t in pv.thumbs.drain(..) {
        unsafe {
            let _ = DwmUnregisterThumbnail(t);
        }
    }
}

fn hide(app: &AppHandle) {
    let mut pv = pv();
    unregister(&mut pv);
    pv.shown = false;
    pv.windows.clear();
    if let Some(Ok(h)) = app.get_webview_window("preview").map(|w| w.hwnd()) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

fn show(app: &AppHandle, group: &str, icon_cx: f64) -> Result<(), String> {
    let live: Vec<isize> = tracker::group_state(group)
        .map(|g| g.windows)
        .unwrap_or_default()
        .into_iter()
        .filter(|&h| unsafe { IsWindow(Some(HWND(h as *mut _))).as_bool() })
        .take(MAX_WINDOWS)
        .collect();
    if live.is_empty() {
        hide(app);
        return Ok(());
    }
    let cfg = crate::config::Config::load(&crate::config::config_path());
    *BRING.lock().unwrap_or_else(|e| e.into_inner()) = (cfg.bring_to_main, cfg.bring_key);
    spawn_key_watch(app.clone());
    let win = ensure(app)?;
    let me = win.hwnd().map_err(|e| e.to_string())?;
    let dock = crate::shell::dock_hwnd();
    let dpi = unsafe { GetDpiForWindow(dock) }.max(96);
    let k = dpi as f64 / 96.0;
    let px = |v: i32| (v as f64 * k).round() as i32;
    let lay = layout(live.len());

    let mut dr = RECT::default();
    unsafe { GetWindowRect(dock, &mut dr) }.map_err(|e| e.to_string())?;
    let size = (px(lay.w), px(lay.h));
    let (x, y) = place(size, dr.left + (icon_cx * crate::shell::dock_k()).round() as i32, dr.top - px(ABOVE_DOCK_CSS), crate::shell::primary_monitor());

    let items: Vec<Item> = live
        .iter()
        .zip(&lay.cells)
        .map(|(&h, &(cx, cy))| {
            let hw = HWND(h as *mut _);
            let mut buf = [0u16; 256];
            let n = unsafe { GetWindowTextW(hw, &mut buf) };
            Item { hwnd: h.to_string(), title: String::from_utf16_lossy(&buf[..n as usize]), minimized: unsafe { IsIconic(hw).as_bool() }, x: cx, y: cy }
        })
        .collect();
    let payload = Payload { items: items.clone(), icon: tracker::icon_of(group), thumb_w: THUMB.0, thumb_h: THUMB.1, title_h: TITLE_H };

    let mut pv = pv();
    unregister(&mut pv);
    (pv.shown, pv.group, pv.icon_cx, pv.windows) = (true, group.to_string(), icon_cx, live.clone());
    pv.payload = Some(payload.clone());
    let _ = app.emit_to("preview", "preview://items", payload);
    unsafe {
        let _ = SetWindowPos(me, Some(HWND_TOPMOST), x, y, size.0, size.1, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        for (it, &src) in items.iter().zip(&live).filter(|(it, _)| !it.minimized) {
            let Ok(t) = DwmRegisterThumbnail(me, HWND(src as *mut _)) else { continue };
            pv.thumbs.push(t);
            let s = DwmQueryThumbnailSourceSize(t).map_or((0, 0), |s| (s.cx, s.cy));
            let (w, h) = fit(s, (px(THUMB.0), px(THUMB.1)));
            let (l, top) = (px(it.x) + (px(THUMB.0) - w) / 2, px(it.y + TITLE_H) + (px(THUMB.1) - h) / 2);
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY,
                rcDestination: RECT { left: l, top, right: l + w, bottom: top + h },
                opacity: 255,
                fVisible: BOOL(1),
                ..Default::default()
            };
            let _ = DwmUpdateThumbnailProperties(t, &props);
        }
    }
    Ok(())
}

/// Pointer rests on an app icon (`icon_cx`: its center x in the dock's CSS px).
#[tauri::command]
pub async fn preview_hover(app: AppHandle, group_id: String, icon_cx: f64) {
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let shown = pv().shown;
    after(if shown { SWITCH_MS } else { DWELL_MS }, g, move || {
        if let Err(e) = show(&app, &group_id, icon_cx) {
            log::warn!("[preview] show failed: {e}");
        }
    });
}

/// Pointer left an icon or the popup: hide unless something re-enters within the grace period.
#[tauri::command]
pub async fn preview_leave(app: AppHandle) {
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    after(GRACE_MS, g, move || hide(&app));
}

/// Pointer entered the popup: cancel a pending hide.
#[tauri::command]
pub async fn preview_enter() {
    GEN.fetch_add(1, Ordering::SeqCst);
}

/// Current popup contents; lets a freshly created preview webview catch up with the first show.
#[tauri::command]
pub fn preview_items() -> Option<Payload> {
    pv().payload.clone()
}

/// Only handles listed in the popup are accepted.
fn listed(hwnd: &str) -> Option<isize> {
    let h: isize = hwnd.parse().ok()?;
    pv().windows.contains(&h).then_some(h)
}

#[tauri::command]
pub async fn activate_window(app: AppHandle, hwnd: String) {
    if let Some(h) = listed(&hwnd) {
        GEN.fetch_add(1, Ordering::SeqCst);
        let _ = tauri::async_runtime::spawn_blocking(move || {
            tracker::activate(h);
            hide(&app);
        })
        .await;
    }
}

#[tauri::command]
pub async fn close_window(app: AppHandle, hwnd: String) {
    let Some(h) = listed(&hwnd) else { return };
    unsafe {
        let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    // Let the tracker notice the closed window, then redraw the list (or hide when it was the last).
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    after(400, g, move || {
        let (group, cx) = {
            let p = pv();
            (p.group.clone(), p.icon_cx)
        };
        let _ = show(&app, &group, cx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_one_and_wrapping() {
        let l = layout(1);
        assert_eq!((l.w, l.h, l.cells), (220, 162, vec![(10, 10)]));
        let l = layout(6);
        assert_eq!((l.w, l.h), (1052, 312));
        assert_eq!(l.cells[5], (10, 10 + 142 + 8));
        assert_eq!(l.cells[4].0, 10 + 4 * 208);
    }

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit((1920, 1080), (200, 120)), (200, 113));
        assert_eq!(fit((500, 1000), (200, 120)), (60, 120));
        assert_eq!(fit((0, 0), (200, 120)), (200, 120));
    }

    #[test]
    fn place_centers_and_clamps() {
        let mon = RECT { left: 0, top: 0, right: 2560, bottom: 1440 };
        assert_eq!(place((200, 100), 1000, 1300, mon), (900, 1200));
        assert_eq!(place((200, 100), 20, 1300, mon).0, 0);
        assert_eq!(place((200, 100), 2550, 1300, mon).0, 2360);
        assert_eq!(place((200, 100), 100, 50, mon).1, 0);
    }
}
