//! Volume mixer popup: per-app audio sessions (Core Audio) and the window that shows them.
use crate::apps::icons;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND, RECT},
        Media::Audio::{
            AudioSessionStateActive, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
            ISimpleAudioVolume, MMDeviceEnumerator, eConsole, eRender,
        },
        System::{
            Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
            Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
        },
        UI::HiDpi::GetDpiForWindow,
        UI::WindowsAndMessaging::*,
    },
    core::{Interface, PWSTR},
};

const W_CSS: i32 = 340;
const HEAD_CSS: i32 = 76; // master row + divider + padding
const ROW_CSS: i32 = 52;
const MAX_ROWS: usize = 8;
const ABOVE_DOCK_CSS: i32 = 8;

static OPEN: AtomicBool = AtomicBool::new(false);
static ICON_CX: AtomicI32 = AtomicI32::new(0); // physical px, screen coordinates

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MixerApp {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub volume: u8,
    pub muted: bool,
}

fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

fn display_name(id: &str) -> String {
    if id == "system" {
        return "Sistem sesleri".into();
    }
    let file = id.rsplit('\\').next().unwrap_or(id);
    let stem = file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file);
    let mut c = stem.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Every non-expired session of the default output device as (key, volume handle); `active_only`
/// keeps just the ones playing sound right now.
/// Key: the process exe path, or "system" for the Windows system sounds.
fn sessions(active_only: bool) -> windows::core::Result<Vec<(String, ISimpleAudioVolume)>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let mgr: IAudioSessionManager2 = en.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate(CLSCTX_ALL, None)?;
        let list = mgr.GetSessionEnumerator()?;
        let mut out = Vec::new();
        for i in 0..list.GetCount()? {
            let Ok(ctl) = list.GetSession(i) else { continue };
            if ctl.GetState().map_or(true, |s| s == AudioSessionStateExpired || (active_only && s != AudioSessionStateActive)) {
                continue;
            }
            let Ok(c2) = ctl.cast::<IAudioSessionControl2>() else { continue };
            let key = if c2.IsSystemSoundsSession().0 == 0 {
                "system".to_string()
            } else {
                match c2.GetProcessId().ok().and_then(exe_of) {
                    Some(p) => p,
                    None => continue,
                }
            };
            if let Ok(v) = ctl.cast::<ISimpleAudioVolume>() {
                out.push((key, v));
            }
        }
        Ok(out)
    }
}

fn list_blocking() -> Vec<MixerApp> {
    let mut apps: Vec<MixerApp> = Vec::new();
    for (id, v) in sessions(true).unwrap_or_default() {
        if apps.iter().any(|a| a.id.eq_ignore_ascii_case(&id)) {
            continue; // several sessions of one app share a row; the first one reports
        }
        let (vol, muted) =
            unsafe { (v.GetMasterVolume().unwrap_or(0.0), v.GetMute().map(|m| m.as_bool()).unwrap_or(false)) };
        let icon = if id == "system" { String::new() } else { icons::icon_for(&id, None) };
        apps.push(MixerApp { name: display_name(&id), icon, id, volume: (vol * 100.0).round() as u8, muted });
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

#[tauri::command]
pub async fn mixer_list() -> Vec<MixerApp> {
    tauri::async_runtime::spawn_blocking(list_blocking).await.unwrap_or_default()
}

#[tauri::command]
pub async fn mixer_set_volume(id: String, value: u8) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        for (k, v) in sessions(false).map_err(|e| e.to_string())? {
            if k.eq_ignore_ascii_case(&id) {
                unsafe { v.SetMasterVolume(f32::from(value.min(100)) / 100.0, std::ptr::null()) }.map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mixer_set_mute(id: String, muted: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        for (k, v) in sessions(false).map_err(|e| e.to_string())? {
            if k.eq_ignore_ascii_case(&id) {
                unsafe { v.SetMute(muted, std::ptr::null()) }.map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Builds the (hidden) popup ahead of its first use, so opening it does not show an unpainted window.
pub fn warm(app: &AppHandle) {
    let _ = ensure(app);
}

fn ensure(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("mixer") {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, "mixer", WebviewUrl::App("index.html#mixer".into()))
        .title("")
        .additional_browser_args(crate::BROWSER_ARGS)
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .inner_size(W_CSS as f64, 200.0)
        .build()
        .map_err(|e| e.to_string())?;
    let h = w.hwnd().map_err(|e| e.to_string())?.0 as isize;
    w.run_on_main_thread(move || crate::nocaption::apply(h))
    .map_err(|e| e.to_string())?;
    let app2 = app.clone();
    w.on_window_event(move |e| {
        if matches!(e, WindowEvent::Focused(false)) {
            hide(&app2);
        }
    });
    Ok(w)
}

/// Starts the page's fade-out, then hides the window once it has played (unless re-opened meanwhile).
fn hide(app: &AppHandle) {
    OPEN.store(false, Ordering::SeqCst);
    let _ = app.emit("mixer://open", false);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(190));
        if OPEN.load(Ordering::SeqCst) {
            return;
        }
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(Ok(h)) = a.get_webview_window("mixer").map(|w| w.hwnd()) {
                if !OPEN.load(Ordering::SeqCst) {
                    unsafe {
                        let _ = ShowWindow(h, SW_HIDE);
                    }
                }
            }
        });
    });
}

/// Places the popup above the dock, centered on the stored icon x, sized for `rows` apps.
fn place(app: &AppHandle, rows: usize, activate: bool) -> Result<(), String> {
    let win = ensure(app)?;
    let me = win.hwnd().map_err(|e| e.to_string())?;
    let dock = crate::shell::dock_hwnd();
    let k = unsafe { GetDpiForWindow(dock) }.max(96) as f64 / 96.0;
    let px = |v: i32| (v as f64 * k).round() as i32;
    let (w, h) = (px(W_CSS), px(HEAD_CSS + ROW_CSS * rows.min(MAX_ROWS) as i32));
    let mut dr = RECT::default();
    unsafe { GetWindowRect(dock, &mut dr) }.map_err(|e| e.to_string())?;
    let mon = crate::shell::primary_monitor();
    let x = (ICON_CX.load(Ordering::SeqCst) - w / 2).min(mon.right - w - px(8)).max(mon.left + px(8));
    let y = crate::shell::popup_y(dr, h, px(ABOVE_DOCK_CSS), mon);
    unsafe {
        let flags = if activate { SWP_SHOWWINDOW } else { SWP_NOACTIVATE };
        let _ = SetWindowPos(me, Some(HWND_TOPMOST), x, y, w, h, flags);
        if activate {
            let _ = SetForegroundWindow(me);
        }
    }
    Ok(())
}

/// Volume icon clicked. `icon_cx`: its center x in the dock's CSS px; `rows`: apps listed now.
#[tauri::command]
pub async fn mixer_toggle(app: AppHandle, icon_cx: f64, rows: usize, was_open: bool) -> Result<(), String> {
    // Clicking the dock blurs the popup (which hides it) before this click arrives, so openness is judged at pointer-down.
    if was_open || OPEN.load(Ordering::SeqCst) {
        let a = app.clone();
        return app.run_on_main_thread(move || hide(&a)).map_err(|e| e.to_string());
    }
    let dock = crate::shell::dock_hwnd();
    let k = unsafe { GetDpiForWindow(dock) }.max(96) as f64 / 96.0;
    let mut dr = RECT::default();
    unsafe { GetWindowRect(dock, &mut dr) }.map_err(|e| e.to_string())?;
    ICON_CX.store(dr.left + (icon_cx * crate::shell::dock_k()).round() as i32, Ordering::SeqCst);
    ensure(&app)?; // build off the main thread: creating a window from inside a main-thread closure deadlocks
    OPEN.store(true, Ordering::SeqCst);
    let a = app.clone();
    app.run_on_main_thread(move || {
        if place(&a, rows, true).is_err() {
            OPEN.store(false, Ordering::SeqCst);
        } else {
            // After the window is on screen, so the page's open animation is visible.
            let _ = a.emit("mixer://open", true);
        }
    })
    .map_err(|e| e.to_string())
}

/// The popup lists a different number of apps than it was sized for.
#[tauri::command]
pub async fn mixer_fit(app: AppHandle, rows: usize) -> Result<(), String> {
    if !OPEN.load(Ordering::SeqCst) {
        return Ok(());
    }
    let a = app.clone();
    app.run_on_main_thread(move || {
        let _ = place(&a, rows, false);
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(display_name(r"C:\Apps\spotify.exe"), "Spotify");
        assert_eq!(display_name("system"), "Sistem sesleri");
    }
    #[test]
    #[ignore]
    fn live_list() {
        println!("{:#?}", list_blocking().iter().map(|a| (&a.name, a.volume, a.muted, a.icon.len())).collect::<Vec<_>>());
    }
}
