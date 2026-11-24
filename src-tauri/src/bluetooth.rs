//! Bluetooth widget: the radio switch, the paired devices with their connection and battery, and the popup
//! that shows them. Devices come from the PnP tree (the same data Windows' Settings page reads), the switch
//! from the Windows.Devices.Radios API. Which devices show on the dock, and with which icon, is the user's.
use crate::config::BtPref;
use crate::settings::{current, save_and_emit};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use windows::{
    Devices::Radios::{Radio, RadioKind, RadioState},
    Win32::{
        Devices::DeviceAndDriverInstallation::*,
        Devices::Properties::DEVPROPTYPE,
        Foundation::DEVPROPKEY,
        UI::HiDpi::GetDpiForWindow,
        UI::WindowsAndMessaging::*,
    },
    core::{GUID, PCWSTR, w},
};

const W_CSS: i32 = 340;
const ABOVE_DOCK_CSS: i32 = 8;

static OPEN: AtomicBool = AtomicBool::new(false);
/// When the popup last closed: clicking the widget blurs (and so closes) the popup before the click arrives,
/// so a close a moment ago means this click is the one that closes it.
static CLOSED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
/// The content height (CSS px) the page last asked for; the popup opens at that size.
static LAST_H: AtomicI32 = AtomicI32::new(0);
static ICON_CX: AtomicI32 = AtomicI32::new(0); // physical px, along the dock

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BtDevice {
    /// The device's name; also the key of the user's choices for it.
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub battery: Option<u8>,
    /// Shown on the dock (the user's choice; default on).
    pub show: bool,
    /// Icon key chosen by the user, or a guess from the name.
    pub icon: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BtState {
    /// A Bluetooth radio exists.
    pub available: bool,
    pub on: bool,
    pub devices: Vec<BtDevice>,
}

const fn key(a: u32, b: u16, c: u16, d: [u8; 8], pid: u32) -> DEVPROPKEY {
    DEVPROPKEY { fmtid: GUID { data1: a, data2: b, data3: c, data4: d }, pid }
}
const FRIENDLY_NAME: DEVPROPKEY = key(0xa45c254e, 0xdf1c, 0x4efd, [0x80, 0x20, 0x67, 0xd1, 0x46, 0xa8, 0x50, 0xe0], 14);
const BATTERY: DEVPROPKEY = key(0x104EA319, 0x6EE2, 0x4701, [0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5], 2);
const CONNECTED: DEVPROPKEY = key(0x83DA6326, 0x97A6, 0x4088, [0x94, 0x53, 0xA1, 0x92, 0x3F, 0x57, 0x3B, 0x29], 15);

fn prop(dev: u32, k: &DEVPROPKEY) -> Option<Vec<u8>> {
    let mut ty = DEVPROPTYPE(0);
    let mut size = 0u32;
    unsafe { CM_Get_DevNode_PropertyW(dev, k, &mut ty, None, &mut size, 0) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let r = unsafe { CM_Get_DevNode_PropertyW(dev, k, &mut ty, Some(buf.as_mut_ptr()), &mut size, 0) };
    (r == CR_SUCCESS).then_some(buf)
}

fn text(b: Vec<u8>) -> String {
    let w: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
    String::from_utf16_lossy(&w)
}

/// Instance ids under one PnP enumerator ("BTHENUM", "BTHLE").
fn instances(enumerator: PCWSTR) -> Vec<Vec<u16>> {
    let mut len = 0u32;
    if unsafe { CM_Get_Device_ID_List_SizeW(&mut len, enumerator, CM_GETIDLIST_FILTER_ENUMERATOR) } != CR_SUCCESS || len < 2 {
        return vec![];
    }
    let mut buf = vec![0u16; len as usize];
    if unsafe { CM_Get_Device_ID_ListW(enumerator, &mut buf, CM_GETIDLIST_FILTER_ENUMERATOR) } != CR_SUCCESS {
        return vec![];
    }
    buf.split(|&c| c == 0).filter(|s| !s.is_empty()).map(|s| s.iter().copied().chain([0]).collect()).collect()
}

/// A raw device node: its name, whether it is connected and its battery, as the PnP tree reports them.
struct Node {
    name: String,
    connected: bool,
    battery: Option<u8>,
}

fn nodes() -> Vec<Node> {
    let mut out = Vec::new();
    for en in [w!("BTHENUM"), w!("BTHLE")] {
        for id in instances(en) {
            let s = String::from_utf16_lossy(&id[..id.len() - 1]).to_uppercase();
            // The device itself ("DEV_<address>"); its services and profiles are other nodes.
            if !(s.starts_with("BTHENUM\\DEV_") || s.starts_with("BTHLE\\DEV_")) {
                continue;
            }
            let mut dev = 0u32;
            if unsafe { CM_Locate_DevNodeW(&mut dev, PCWSTR(id.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) } != CR_SUCCESS {
                continue;
            }
            let Some(name) = prop(dev, &FRIENDLY_NAME).map(text).filter(|n| !n.is_empty()) else { continue };
            let connected = prop(dev, &CONNECTED).is_some_and(|b| b.first().is_some_and(|&v| v != 0));
            let battery = prop(dev, &BATTERY).and_then(|b| b.first().copied()).filter(|&p| p <= 100);
            out.push(Node { name, connected, battery });
        }
    }
    out
}

/// Icon key guessed from a device name; the user can pick another.
pub fn guess_icon(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| n.contains(w));
    if has(&["buds", "airpods", "earbud", "pods", "tws"]) {
        "earbuds"
    } else if has(&["headset", "headphone", "kulaklık", "kulaklik", "wh-", "wf-", "arctis", "pro x", "g pro"]) {
        "headphones"
    } else if has(&["keyboard", "klavye", "neo", "k380", "mx keys", "kb"]) {
        "keyboard"
    } else if has(&["mouse", "fare", "mx master", "g502", "g305", "g703"]) {
        "mouse"
    } else if has(&["gamepad", "controller", "xbox", "dualsense", "dualshock", "joy-con"]) {
        "gamepad"
    } else if has(&["speaker", "hoparlör", "jbl", "soundbar", "boom", "flip", "charge"]) {
        "speaker"
    } else if has(&["watch", "band", "saat"]) {
        "watch"
    } else if has(&["iphone", "galaxy s", "pixel", "phone", "telefon", "redmi", "xiaomi"]) {
        "phone"
    } else {
        "bluetooth"
    }
}

/// Devices merged by name (a pair of earbuds or a classic and an LE node of one device are several nodes),
/// connected ones first.
fn devices(prefs: &std::collections::HashMap<String, BtPref>) -> Vec<BtDevice> {
    let mut v: Vec<BtDevice> = Vec::new();
    for n in nodes() {
        match v.iter_mut().find(|d| d.id == n.name) {
            Some(d) => {
                d.connected |= n.connected;
                d.battery = d.battery.or(n.battery);
            }
            None => {
                let p = prefs.get(&n.name);
                v.push(BtDevice {
                    show: p.is_none_or(|p| p.show),
                    icon: p.map(|p| p.icon.clone()).filter(|i| !i.is_empty()).unwrap_or_else(|| guess_icon(&n.name).into()),
                    name: n.name.clone(),
                    id: n.name,
                    connected: n.connected,
                    battery: n.battery,
                });
            }
        }
    }
    // A device that is not connected has no current battery; do not show a stale value.
    for d in &mut v {
        if !d.connected {
            d.battery = None;
        }
    }
    v.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

fn radio() -> Option<Radio> {
    let list = Radio::GetRadiosAsync().ok()?.join().ok()?;
    list.into_iter().find(|r| r.Kind().ok() == Some(RadioKind::Bluetooth))
}

fn state_blocking() -> BtState {
    let prefs = current().map(|c| c.bt_prefs).unwrap_or_default();
    let r = radio();
    BtState { available: r.is_some(), on: r.is_some_and(|r| r.State().ok() == Some(RadioState::On)), devices: devices(&prefs) }
}

#[tauri::command]
pub async fn bt_get() -> BtState {
    tauri::async_runtime::spawn_blocking(state_blocking).await.unwrap_or_default()
}

/// Switch the Bluetooth radio.
#[tauri::command]
pub async fn bt_set_power(app: AppHandle, on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let r = radio().ok_or("Bluetooth bulunamadı")?;
        let st = if on { RadioState::On } else { RadioState::Off };
        r.SetStateAsync(st).and_then(|a| a.join()).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit("bt://changed", ());
    Ok(())
}

/// Remember whether a device shows on the dock and which icon it has.
#[tauri::command]
pub async fn bt_set_pref(app: AppHandle, id: String, show: bool, icon: String) -> Result<(), String> {
    let mut c = current()?;
    let icon = if ICONS.contains(&icon.as_str()) { icon } else { String::new() };
    // Tell the widgets first, so they animate at once; the full state follows once it is saved.
    let _ = app.emit("bt://pref", serde_json::json!({ "id": id, "show": show, "icon": icon }));
    c.bt_prefs.insert(id, BtPref { show, icon });
    save_and_emit(&app, &c)?;
    let _ = app.emit("bt://changed", ());
    Ok(())
}

const ICONS: [&str; 9] = ["bluetooth", "headphones", "earbuds", "keyboard", "mouse", "gamepad", "speaker", "phone", "watch"];

// ---- popup window (the same pattern as the volume mixer) ----

fn ensure(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("bt") {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, "bt", WebviewUrl::App("index.html#bt".into()))
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
    w.run_on_main_thread(move || crate::nocaption::apply(h)).map_err(|e| e.to_string())?;
    let app2 = app.clone();
    w.on_window_event(move |e| {
        if matches!(e, WindowEvent::Focused(false)) {
            hide(&app2);
        }
    });
    Ok(w)
}

/// Builds the (hidden) popup ahead of its first use.
pub fn warm(app: &AppHandle) {
    let _ = ensure(app);
}

fn hide(app: &AppHandle) {
    if OPEN.swap(false, Ordering::SeqCst) {
        *CLOSED_AT.lock().unwrap_or_else(|e| e.into_inner()) = Some(std::time::Instant::now());
    }
    let _ = app.emit("bt://open", false);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(190));
        if OPEN.load(Ordering::SeqCst) {
            return;
        }
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(Ok(h)) = a.get_webview_window("bt").map(|w| w.hwnd()) {
                if !OPEN.load(Ordering::SeqCst) {
                    unsafe {
                        let _ = ShowWindow(h, SW_HIDE);
                    }
                }
            }
        });
    });
}

/// Next to the dock, centered on the stored icon position, `height_css` tall.
fn place(app: &AppHandle, height_css: f64, activate: bool) -> Result<(), String> {
    let win = ensure(app)?;
    let me = win.hwnd().map_err(|e| e.to_string())?;
    let dock = crate::shell::dock_hwnd();
    let k = unsafe { GetDpiForWindow(dock) }.max(96) as f64 / 96.0;
    let px = |v: f64| (v * k).round() as i32;
    let (w, h) = (px(W_CSS as f64), px(height_css.clamp(120.0, 900.0)));
    let mut dr = windows::Win32::Foundation::RECT::default();
    unsafe { GetWindowRect(dock, &mut dr) }.map_err(|e| e.to_string())?;
    let mon = crate::shell::primary_monitor();
    let (x, y) = crate::shell::popup_pos(dr, ICON_CX.load(Ordering::SeqCst), (w, h), px(ABOVE_DOCK_CSS as f64), px(8.0), mon);
    unsafe {
        let flags = if activate { SWP_SHOWWINDOW } else { SWP_NOACTIVATE };
        let _ = SetWindowPos(me, Some(HWND_TOPMOST), x, y, w, h, flags);
        if activate {
            let _ = SetForegroundWindow(me);
        }
    }
    Ok(())
}

/// The widget was clicked. `icon_cx`: its center along the dock in the dock's CSS px.
#[tauri::command]
pub async fn bt_toggle(app: AppHandle, icon_cx: f64, height: f64, was_open: bool) -> Result<(), String> {
    let _ = was_open; // the page's idea of "open" can go stale; the backend knows
    let just_closed = CLOSED_AT.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(400));
    if just_closed || OPEN.load(Ordering::SeqCst) {
        *CLOSED_AT.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let a = app.clone();
        return app.run_on_main_thread(move || hide(&a)).map_err(|e| e.to_string());
    }
    let mut dr = windows::Win32::Foundation::RECT::default();
    unsafe { GetWindowRect(crate::shell::dock_hwnd(), &mut dr) }.map_err(|e| e.to_string())?;
    ICON_CX.store(crate::shell::along_pos(dr, icon_cx), Ordering::SeqCst);
    ensure(&app)?;
    OPEN.store(true, Ordering::SeqCst);
    let a = app.clone();
    let height = match LAST_H.load(Ordering::SeqCst) {
        0 => height,
        h => h as f64,
    };
    app.run_on_main_thread(move || {
        if place(&a, height, true).is_err() {
            OPEN.store(false, Ordering::SeqCst);
        } else {
            let _ = a.emit("bt://open", true);
        }
    })
    .map_err(|e| e.to_string())
}

/// The page's content changed height.
#[tauri::command]
pub async fn bt_fit(app: AppHandle, height: f64) -> Result<(), String> {
    LAST_H.store(height.round() as i32, Ordering::SeqCst);
    if !OPEN.load(Ordering::SeqCst) {
        return Ok(());
    }
    let a = app.clone();
    app.run_on_main_thread(move || {
        let _ = place(&a, height, false);
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icon_guesses() {
        assert_eq!(guess_icon("Galaxy Buds3 (CB4F)"), "earbuds");
        assert_eq!(guess_icon("NEO87 BT1"), "keyboard");
        assert_eq!(guess_icon("Sony WH-1000XM5"), "headphones");
        assert_eq!(guess_icon("Something"), "bluetooth");
    }
    #[test]
    #[ignore]
    fn live() {
        println!("{:#?}", state_blocking());
    }
}
