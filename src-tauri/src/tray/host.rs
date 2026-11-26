//! Tray icons. Spike result: a real Shell_TrayWnd host is not viable while Explorer runs (see
//! docs/superpowers/research/tray.md), so the active path is the fallback: `open_tray_overflow`
//! shows Windows' own overflow flyout. The pure NOTIFYICONDATA mapping below is kept (tested) for
//! the day a host is feasible; `tray://changed` carries an empty list until then.
use crate::state::Hub;
use serde::Serialize;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TrayIcon {
    pub id: String,
    pub tooltip: String,
    pub icon_data_url: String,
}

pub const NIF_ICON: u32 = 0x2;
pub const NIF_TIP: u32 = 0x4;

/// NOTIFYICONDATA -> TrayIcon. Fields whose NIF_* flag is unset keep the previous value (NIM_MODIFY
/// semantics). `tip` is the raw UTF-16 buffer, NUL-terminated or full.
pub fn map_notify(id: &str, flags: u32, tip: &[u16], icon_url: Option<String>, prev: Option<&TrayIcon>) -> TrayIcon {
    let old = prev.cloned().unwrap_or_else(|| TrayIcon { id: id.into(), ..Default::default() });
    let end = tip.iter().position(|&c| c == 0).unwrap_or(tip.len());
    TrayIcon {
        id: id.into(),
        tooltip: if flags & NIF_TIP != 0 { String::from_utf16_lossy(&tip[..end]) } else { old.tooltip },
        icon_data_url: match icon_url {
            Some(u) if flags & NIF_ICON != 0 => u,
            _ => old.icon_data_url,
        },
    }
}

pub fn start(app: AppHandle) {
    app.state::<Hub>().emit("tray://changed", Vec::<TrayIcon>::new());
}

/// No host, so there is no owner window to forward to.
#[tauri::command]
pub async fn tray_click(_id: String, _button: String, _x: i32, _y: i32) -> Result<(), String> {
    Err("tray host unavailable; use open_tray_overflow".into())
}

/// The flyout opens next to the (hidden) taskbar; move it above the chevron at physical (x, y).
fn find_flyout() -> Option<windows::Win32::Foundation::HWND> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe extern "system" fn find(h: HWND, out: LPARAM) -> BOOL {
        let mut buf = [0u16; 128];
        let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
        if unsafe { IsWindowVisible(h) }.as_bool() && String::from_utf16_lossy(&buf[..n]).contains("OverflowXamlIsland") {
            unsafe { *(out.0 as *mut isize) = h.0 as isize };
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut found: isize = 0;
    let _ = unsafe { EnumWindows(Some(find), LPARAM(&mut found as *mut isize as isize)) };
    (found != 0).then(|| HWND(found as *mut _))
}

/// The overflow flyout's top-level window by its class: a lookup in the kernel's window list instead of walking
/// every window, so the poll that moves the flyout reacts within a millisecond. `visible_only` false also finds it
/// while it is hidden between uses.
fn flyout_window(visible_only: bool) -> Option<windows::Win32::Foundation::HWND> {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsWindowVisible};
    let h = unsafe { FindWindowW(w!("TopLevelWindowForOverflowXamlIsland"), None) }.ok()?;
    (!visible_only || unsafe { IsWindowVisible(h) }.as_bool()).then_some(h)
}

/// Windows shows a small "show/hide hidden icons" tooltip at the hidden taskbar's chevron
/// (bottom-right corner), far from the dock. Hide it.
fn hide_stray_tooltip() {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe extern "system" fn cb(h: HWND, _: LPARAM) -> BOOL {
        let mut buf = [0u16; 64];
        let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
        if String::from_utf16_lossy(&buf[..n]) == "Xaml_WindowedPopupClass" && unsafe { IsWindowVisible(h) }.as_bool() {
            let mut r = RECT::default();
            if unsafe { GetWindowRect(h, &mut r) }.is_ok() && r.right - r.left < 400 && r.bottom - r.top < 100 {
                let _ = unsafe { ShowWindow(h, SW_HIDE) };
            }
        }
        BOOL(1)
    }
    let _ = unsafe { EnumWindows(Some(cb), LPARAM(0)) };
}

/// Emits `tray://flyout` (bool) so the chevron can rotate; ends when the flyout closes.
fn watch_flyout(app: AppHandle) {
    use tauri::Emitter;
    let _ = app.emit("tray://flyout", true);
    std::thread::spawn(move || {
        while find_flyout().is_some() {
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        let _ = app.emit("tray://flyout", false);
        crate::shell::reassert(); // the flyout's focus change can leave the dock hidden or buried
    });
}

/// Where the flyout belongs: above the chevron at physical (x, y), next to the dock's edge.
fn flyout_spot(x: i32, y: i32, w: i32, ht: i32) -> (i32, i32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    // The page's y is unreliable (window.screenY); the dock window's own rect is exact.
    let mut dr = RECT::default();
    let at = if unsafe { GetWindowRect(crate::shell::dock_hwnd(), &mut dr) }.is_ok() { dr } else { RECT { top: y, bottom: y, ..dr } };
    let along = if crate::shell::vertical() { y } else { x };
    crate::shell::popup_pos(at, along, (w, ht), 12, 0, crate::shell::primary_monitor())
}

fn reposition_flyout(app: &AppHandle, x: i32, y: i32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::*;
    // Tooltip hiding outlives the move, on its own thread, so a new click is not blocked meanwhile.
    std::thread::spawn(|| {
        for _ in 0..700 {
            std::thread::sleep(std::time::Duration::from_millis(4));
            hide_stray_tooltip();
        }
    });
    // Explorer shows the flyout at the hidden taskbar (bottom-right) and only then do we can move it, so every
    // microsecond between its show and our move is a visible frame there. Spin (no sleeping) at high priority.
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL, THREAD_PRIORITY_NORMAL};
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) };
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(3) {
        let Some(h) = flyout_window(true) else {
            std::thread::yield_now();
            continue;
        };
        let mut r = RECT::default();
        if unsafe { GetWindowRect(h, &mut r) }.is_err() { return; }
        let (fx, fy) = flyout_spot(x, y, r.right - r.left, r.bottom - r.top);
        let _ = unsafe { SetWindowPos(h, None, fx, fy, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
        log::info!("[tray] flyout moved {} ms after the keys", started.elapsed().as_millis());
        let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL) };
        watch_flyout(app.clone());
        return;
    }
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL) };
}

/// Win+B focuses the tray's "Show hidden icons" button; Enter opens the flyout.
#[tauri::command]
pub async fn open_tray_overflow(app: AppHandle, x: i32, y: i32, was_open: bool) -> Result<(), String> {
    // The flyout closes itself on an outside click, so "open" is judged at pointer-down (was_open).
    if was_open {
        tauri::async_runtime::spawn_blocking(|| {
            use windows::Win32::UI::Input::KeyboardAndMouse::*;
            if find_flyout().is_some() {
                let k = |up: bool| INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_ESCAPE, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() } } };
                unsafe { SendInput(&[k(false), k(true)], size_of::<INPUT>() as i32) };
            }
        })
        .await
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    // One run at a time: a second click mid-run would send a stray Enter to whatever has focus.
    static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if BUSY.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() } },
        };
        let send = |v: &[INPUT]| unsafe { SendInput(v, size_of::<INPUT>() as i32) };
        // Flyout already open: the button toggles it closed. Enter now would activate its focused icon (Claude).
        if find_flyout().is_some() {
            send(&[key(VK_ESCAPE, false), key(VK_ESCAPE, true)]);
            BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
            return;
        }
        // Windows drops input injected while an elevated window (Task Manager) has the foreground, so
        // take the foreground with the dock first.
        crate::apps::tracker::activate(crate::shell::dock_hwnd().0 as isize);
        std::thread::sleep(std::time::Duration::from_millis(60));
        send(&[key(VK_LWIN, false), key(VK_B, false), key(VK_B, true), key(VK_LWIN, true)]);
        std::thread::sleep(std::time::Duration::from_millis(250));
        send(&[key(VK_RETURN, false), key(VK_RETURN, true)]);
        reposition_flyout(&app, x, y);
        BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tip(s: &str) -> Vec<u16> {
        let mut v: Vec<u16> = s.encode_utf16().collect();
        v.resize(128, 0);
        v
    }
    #[test]
    fn add_with_tip_and_icon() {
        let t = map_notify("a:1", NIF_TIP | NIF_ICON, &tip("Discord"), Some("data:x".into()), None);
        assert_eq!(t, TrayIcon { id: "a:1".into(), tooltip: "Discord".into(), icon_data_url: "data:x".into() });
    }
    #[test]
    fn modify_without_flags_keeps_previous() {
        let prev = TrayIcon { id: "a:1".into(), tooltip: "old".into(), icon_data_url: "data:old".into() };
        let t = map_notify("a:1", 0, &tip("ignored"), Some("data:new".into()), Some(&prev));
        assert_eq!(t, prev);
    }
    #[test]
    fn tip_only_and_icon_only() {
        let prev = TrayIcon { id: "a:1".into(), tooltip: "old".into(), icon_data_url: "data:old".into() };
        let t = map_notify("a:1", NIF_TIP, &tip("new"), Some("data:new".into()), Some(&prev));
        assert_eq!((t.tooltip.as_str(), t.icon_data_url.as_str()), ("new", "data:old"));
        let t = map_notify("a:1", NIF_ICON, &tip("new"), Some("data:new".into()), Some(&prev));
        assert_eq!((t.tooltip.as_str(), t.icon_data_url.as_str()), ("old", "data:new"));
    }
    #[test]
    fn unterminated_tip() {
        let full = vec![b'a' as u16; 128];
        assert_eq!(map_notify("i", NIF_TIP, &full, None, None).tooltip.len(), 128);
    }
}
