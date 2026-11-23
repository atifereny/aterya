//! Keyboard layout picker: a popup above the dock listing the installed layouts. Picking one switches the
//! window that had the focus before the popup opened (Alt+Shift keeps working on its own).
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{HWND, LPARAM, RECT, WPARAM},
        Globalization::{GetLocaleInfoW, LOCALE_SISO639LANGNAME, LOCALE_SNATIVEDISPLAYNAME},
        Media::Audio::{DEVICE_STATE_ACTIVE, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender},
        System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ},
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardLayoutList, HKL},
            WindowsAndMessaging::*,
        },
    },
    core::{GUID, HRESULT, PCWSTR, PWSTR},
};

const W_CSS: i32 = 250;
const W_AUDIO_CSS: i32 = 340;
const PAD_CSS: i32 = 12;
const ROW_CSS: i32 = 39; // 38 row + 1 gap
const SEP_CSS: i32 = 8; // extra height of a separator row
const SEP_KEYS: [&str; 6] = ["pin", "unpin", "taskmgr", "desktop", "recent:0", "clear"];
static SEPS: AtomicI32 = AtomicI32::new(0);
const MAX_ROWS: usize = 16;
const ABOVE_DOCK_CSS: i32 = 8;
const MSG_INPUTLANGCHANGEREQUEST: u32 = 0x0050;

static OPEN: AtomicBool = AtomicBool::new(false);
/// The popup lists output devices instead of keyboard layouts.
static AUDIO: AtomicBool = AtomicBool::new(false);
/// The popup is the right-click menu of the dock app with this id (empty: not in that mode).
static APP_MENU: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// The Win+X menu: (key, label, what `start` opens). "runas:" opens it elevated (Windows asks for consent).
const WINX: &[(&str, &str, &str)] = &[
    ("apps", "Yüklü Uygulamalar", "ms-settings:appsfeatures"),
    ("mobility", "Mobilite Merkezi", "mblctr.exe"),
    ("power", "Güç Seçenekleri", "ms-settings:powersleep"),
    ("events", "Olay Görüntüleyicisi", "eventvwr.exe"),
    ("system", "Sistem", "ms-settings:about"),
    ("devices", "Aygıt Yöneticisi", "devmgmt.msc"),
    ("network", "Ağ Bağlantıları", "ms-settings:network-status"),
    ("disks", "Disk Yönetimi", "diskmgmt.msc"),
    ("compmgmt", "Bilgisayar Yönetimi", "compmgmt.msc"),
    ("term", "Terminal", "wt.exe"),
    ("termadmin", "Terminal (Yönetici)", "runas:wt.exe"),
    ("taskmgr", "Görev Yöneticisi", "taskmgr.exe"),
    ("settings", "Ayarlar", "ms-settings:"),
    ("explorer", "Dosya Gezgini", "explorer.exe"),
    ("run", "Çalıştır", "shell:::{2559a1f3-21d7-11d4-bdaf-00c04f60b9f0}"),
    ("desktop", "Masaüstü", "shell:::{3080F90D-D7AD-11D9-BD98-0000947B0257}"),
];

fn winx_run(key: &str) {
    use std::os::windows::process::CommandExt;
    let Some((_, _, target)) = WINX.iter().find(|(k, _, _)| *k == key) else { return };
    let mut c = if let Some(exe) = target.strip_prefix("runas:") {
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &format!("Start-Process {exe} -Verb RunAs")]);
        c
    } else if target.starts_with("shell:::") {
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(target);
        c
    } else {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", target]);
        c
    };
    let _ = c.creation_flags(0x0800_0000).spawn(); // CREATE_NO_WINDOW
}

/// Recent files of the app whose menu is open, loaded when the menu opens.
static RECENTS: std::sync::Mutex<Vec<crate::apps::recent::Recent>> = std::sync::Mutex::new(Vec::new());

/// Rows of an app's menu; `hkl` carries the action: "one", "all" or "new".
fn app_items(id: &str) -> Vec<Layout> {
    if id == "clip" {
        let items = crate::clip::history(8);
        if items.is_empty() {
            return vec![Layout { hkl: "none".into(), code: "i:clip".into(), name: "Henüz bir şey kopyalanmadı".into(), current: false }];
        }
        let one_line = |s: &str| {
            let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
            if t.chars().count() > 42 { t.chars().take(41).collect::<String>() + "…" } else { t }
        };
        let mut v: Vec<Layout> = items
            .iter()
            .enumerate()
            .map(|(i, e)| Layout { hkl: format!("clip:{i}"), code: e.thumb.as_ref().map_or("i:clip".to_string(), |t| format!("img:{t}")), name: one_line(&e.label), current: false })
            .collect();
        v.push(Layout { hkl: "clear".into(), code: "i:clear".into(), name: "Geçmişi Temizle".into(), current: false });
        return v;
    }
    if id == "winx" {
        return WINX.iter().map(|(k, name, _)| Layout { hkl: (*k).into(), code: format!("i:{k}"), name: (*name).into(), current: false }).collect();
    }
    let n = crate::apps::tracker::group_state(id).map_or(0, |g| g.windows.len());
    let row = |hkl: &str, name: &str| Layout { hkl: hkl.into(), code: format!("i:{hkl}"), name: name.into(), current: false };
    let mut v = Vec::new();
    if n > 0 {
        v.push(row("one", "Pencereyi Kapat"));
    }
    if n > 1 {
        v.push(row("all", &format!("Tüm Pencereleri Kapat ({n})")));
    }
    v.push(row("new", "Yeni Pencere"));
    for (i, r) in RECENTS.lock().unwrap().iter().enumerate() {
        let name: String = if r.name.chars().count() > 34 { r.name.chars().take(33).collect::<String>() + "…" } else { r.name.clone() };
        v.push(Layout { hkl: format!("recent:{i}"), code: "i:doc".into(), name, current: false });
    }
    let pinned = crate::apps::tracker::entry(id).is_some_and(|e| e.pinned);
    v.push(if pinned { row("unpin", "Sabitlemeyi Kaldır") } else { row("pin", "Çubuğa Sabitle") });
    if n > 0 {
        v.push(row("end", "Görevi Sonlandır"));
    }
    v
}

fn app_action(app: &AppHandle, id: &str, act: &str) {
    if id == "clip" {
        match act {
            "clear" => crate::clip::clear(),
            a if a.starts_with("clip:") => {
                if let Ok(i) = a[5..].parse::<usize>() {
                    crate::clip::copy(i);
                }
            }
            _ => {}
        }
        return;
    }
    if id == "winx" {
        return winx_run(act);
    }
    let wins = crate::apps::tracker::group_state(id).map(|g| g.windows).unwrap_or_default();
    match act {
        "one" => wins.iter().take(1).for_each(|&w| close(w)),
        "all" => wins.iter().for_each(|&w| close(w)),
        "pin" | "unpin" => {
            if let Err(e) = crate::apps::pins::set_pinned(app, id, act == "pin") {
                log::warn!("[menu] pin: {e}");
            }
        }
        "end" => wins.iter().for_each(|&w| end_task(w)),
        a if a.starts_with("recent:") => {
            let item = a[7..].parse::<usize>().ok().and_then(|i| RECENTS.lock().unwrap().get(i).cloned());
            if let Some(r) = item {
                crate::apps::recent::open(&r);
            }
        }
        _ => {
            let _ = crate::apps::pins::launch(id);
        }
    }
}

/// Terminate the process that owns the window (what Task Manager's End task does).
fn end_task(w: isize) {
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(w as *mut _), Some(&mut pid));
        if pid == 0 || pid == std::process::id() {
            return;
        }
        if let Ok(p) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(p, 1);
            let _ = windows::Win32::Foundation::CloseHandle(p);
        }
    }
}

fn close(w: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(w as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
static ICON_CX: AtomicI32 = AtomicI32::new(0); // physical px, screen coordinates
/// The window that had the focus when the popup opened: the one a pick switches.
static TARGET: AtomicIsize = AtomicIsize::new(0);

#[derive(Serialize)]
pub struct Layout {
    /// HKL as a string (it can exceed JavaScript's safe integers).
    hkl: String,
    code: String,
    name: String,
    current: bool,
}

fn locale(langid: u32, kind: u32) -> String {
    let mut b = [0u16; 96];
    let n = unsafe { GetLocaleInfoW(langid, kind, Some(&mut b)) };
    String::from_utf16_lossy(&b[..(n.max(1) - 1) as usize])
}

/// Undocumented but stable COM interface that changes the default audio endpoint (what Sound settings uses).
#[windows::core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: windows::core::IUnknown {
    fn U1(&self) -> HRESULT;
    fn U2(&self) -> HRESULT;
    fn U3(&self) -> HRESULT;
    fn U4(&self) -> HRESULT;
    fn U5(&self) -> HRESULT;
    fn U6(&self) -> HRESULT;
    fn U7(&self) -> HRESULT;
    fn U8(&self) -> HRESULT;
    fn U9(&self) -> HRESULT;
    fn U10(&self) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: i32) -> HRESULT;
}

fn take(p: PWSTR) -> String {
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as _)) };
    s
}

/// Active output devices; the default one is marked current.
fn devices() -> Vec<Layout> {
    let mut out = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return out };
        let cur = en.GetDefaultAudioEndpoint(eRender, eConsole).ok().and_then(|d| d.GetId().ok()).map(take);
        let Ok(col) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return out };
        for i in 0..col.GetCount().unwrap_or(0) {
            let Ok(d) = col.Item(i) else { continue };
            let Ok(id) = d.GetId().map(take) else { continue };
            let name = d
                .OpenPropertyStore(STGM_READ)
                .and_then(|s| s.GetValue(&PKEY_Device_FriendlyName))
                .map(|pv| pv.to_string())
                .unwrap_or_else(|_| "Ses aygıtı".into());
            out.push(Layout { current: cur.as_deref() == Some(id.as_str()), hkl: id, code: String::new(), name });
        }
    }
    out
}

fn set_default_device(id: &str) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let pc: IPolicyConfig = CoCreateInstance(&GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9), None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let w: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
        for role in [0, 1, 2] {
            pc.SetDefaultEndpoint(PCWSTR(w.as_ptr()), role).ok().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn lang_list() -> Vec<Layout> {
    let id = APP_MENU.lock().unwrap().clone();
    if !id.is_empty() {
        return app_items(&id);
    }
    if AUDIO.load(Ordering::SeqCst) {
        return devices();
    }
    unsafe {
        let target = HWND(TARGET.load(Ordering::SeqCst) as *mut _);
        let cur = GetKeyboardLayout(GetWindowThreadProcessId(target, None)).0 as isize;
        let n = GetKeyboardLayoutList(None);
        let mut buf = vec![HKL::default(); n.max(0) as usize];
        let n = GetKeyboardLayoutList(Some(&mut buf));
        buf.truncate(n.max(0) as usize);
        buf.iter()
            .map(|h| {
                let id = (h.0 as usize & 0xFFFF) as u32;
                Layout { hkl: (h.0 as isize).to_string(), code: locale(id, LOCALE_SISO639LANGNAME).to_uppercase(), name: locale(id, LOCALE_SNATIVEDISPLAYNAME), current: h.0 as isize == cur }
            })
            .collect()
    }
}

/// Builds the (hidden) popup ahead of its first use, so opening it does not show an unpainted window.
pub fn warm(app: &AppHandle) {
    let _ = ensure(app);
}

fn ensure(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("langpop") {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, "langpop", WebviewUrl::App("index.html#lang".into()))
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
    let _ = app.emit("lang://open", false);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(190));
        if OPEN.load(Ordering::SeqCst) {
            return;
        }
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(Ok(h)) = a.get_webview_window("langpop").map(|w| w.hwnd()) {
                if !OPEN.load(Ordering::SeqCst) {
                    unsafe {
                        let _ = ShowWindow(h, SW_HIDE);
                    }
                }
            }
        });
    });
}

/// Above the dock, centered on the stored icon x, sized for `rows` layouts.
fn place(app: &AppHandle, rows: usize) -> Result<(), String> {
    let win = ensure(app)?;
    let me = win.hwnd().map_err(|e| e.to_string())?;
    let dock = crate::shell::dock_hwnd();
    let k = unsafe { GetDpiForWindow(dock) }.max(96) as f64 / 96.0;
    let px = |v: i32| (v as f64 * k).round() as i32;
    let (w, h) = (px(if AUDIO.load(Ordering::SeqCst) || APP_MENU.lock().map(|m| *m == "clip").unwrap_or(false) { W_AUDIO_CSS } else { W_CSS }), px(PAD_CSS + ROW_CSS * rows.clamp(1, MAX_ROWS) as i32 + SEP_CSS * SEPS.load(Ordering::SeqCst)));
    let mut dr = RECT::default();
    unsafe { GetWindowRect(dock, &mut dr) }.map_err(|e| e.to_string())?;
    let mon = crate::shell::primary_monitor();
    let (x, y) = crate::shell::popup_pos(dr, ICON_CX.load(Ordering::SeqCst), (w, h), px(ABOVE_DOCK_CSS), px(8), mon);
    unsafe {
        let _ = SetWindowPos(me, Some(HWND_TOPMOST), x, y, w, h, SWP_SHOWWINDOW);
        let _ = SetForegroundWindow(me);
    }
    Ok(())
}

/// Language widget clicked. `icon_cx`: its center x in the dock's CSS px; `rows`: layouts installed.
/// `was_open` is judged at pointer-down: clicking the dock blurs (and so hides) the popup before the click arrives.
#[tauri::command]
pub async fn lang_pop_toggle(app: AppHandle, icon_cx: f64, rows: usize, was_open: bool) -> Result<(), String> {
    toggle(app, icon_cx, rows, was_open, false, String::new()).await
}

/// Volume widget right-clicked: same popup, listing the output devices.
#[tauri::command]
pub async fn outdev_pop_toggle(app: AppHandle, icon_cx: f64, was_open: bool) -> Result<(), String> {
    let rows = tauri::async_runtime::spawn_blocking(|| devices().len()).await.unwrap_or(0);
    toggle(app, icon_cx, rows, was_open, true, String::new()).await
}

/// Dock app right-clicked: same popup, listing what can be done to its windows.
#[tauri::command]
pub async fn app_menu_toggle(app: AppHandle, icon_cx: f64, id: String, was_open: bool) -> Result<(), String> {
    if !was_open {
        let id2 = id.clone();
        let list = tauri::async_runtime::spawn_blocking(move || crate::apps::recent::list(&id2, 5)).await.unwrap_or_default();
        *RECENTS.lock().unwrap() = list;
    }
    let rows = app_items(&id).len();
    toggle(app, icon_cx, rows, was_open, false, id).await
}

async fn toggle(app: AppHandle, icon_cx: f64, rows: usize, was_open: bool, audio: bool, menu: String) -> Result<(), String> {
    if was_open || OPEN.load(Ordering::SeqCst) {
        let a = app.clone();
        return app.run_on_main_thread(move || hide(&a)).map_err(|e| e.to_string());
    }
    let dock = crate::shell::dock_hwnd();
    let k = unsafe { GetDpiForWindow(dock) }.max(96) as f64 / 96.0;
    let mut dr = RECT::default();
    unsafe { GetWindowRect(dock, &mut dr) }.map_err(|e| e.to_string())?;
    ICON_CX.store(dr.left + (icon_cx * crate::shell::dock_k()).round() as i32, Ordering::SeqCst);
    TARGET.store(unsafe { GetForegroundWindow() }.0 as isize, Ordering::SeqCst);
    ensure(&app)?; // build off the main thread: creating a window from inside a main-thread closure deadlocks
    AUDIO.store(audio, Ordering::SeqCst);
    SEPS.store(if menu.is_empty() { 0 } else { app_items(&menu).iter().filter(|l| SEP_KEYS.contains(&l.hkl.as_str())).count() as i32 }, Ordering::SeqCst);
    *APP_MENU.lock().unwrap() = menu;
    OPEN.store(true, Ordering::SeqCst);
    let a = app.clone();
    app.run_on_main_thread(move || {
        if place(&a, rows).is_err() {
            OPEN.store(false, Ordering::SeqCst);
        } else {
            let _ = a.emit("lang://open", true); // after the window is on screen, so the open animation is visible
        }
    })
    .map_err(|e| e.to_string())
}

/// Switch the remembered window to the layout `hkl` and close the popup.
#[tauri::command]
pub async fn lang_select(app: AppHandle, hkl: String) -> Result<(), String> {
    let menu = APP_MENU.lock().unwrap().clone();
    if !menu.is_empty() {
        let a = app.clone();
        app.run_on_main_thread(move || hide(&a)).map_err(|e| e.to_string())?;
        let a2 = app.clone();
        std::thread::spawn(move || app_action(&a2, &menu, &hkl));
        return Ok(());
    }
    if AUDIO.load(Ordering::SeqCst) {
        let r = tauri::async_runtime::spawn_blocking(move || set_default_device(&hkl)).await.map_err(|e| e.to_string())?;
        let a = app.clone();
        app.run_on_main_thread(move || hide(&a)).map_err(|e| e.to_string())?;
        return r;
    }
    let h: isize = hkl.parse().map_err(|_| "geçersiz düzen".to_string())?;
    let target = TARGET.load(Ordering::SeqCst);
    unsafe {
        let _ = PostMessageW(Some(HWND(target as *mut _)), MSG_INPUTLANGCHANGEREQUEST, WPARAM(0), LPARAM(h));
    }
    let a = app.clone();
    app.run_on_main_thread(move || {
        hide(&a);
        unsafe {
            let _ = SetForegroundWindow(HWND(target as *mut _)); // give the focus back to where the user was typing
        }
    })
    .map_err(|e| e.to_string())
}
