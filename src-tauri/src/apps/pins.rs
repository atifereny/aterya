use super::click::{decide_click, ClickAction};
use super::tracker;
use windows::{
    core::{HSTRING, PCWSTR},
    Win32::{
        Foundation::HWND,
        UI::{
            Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LWIN},
            Shell::ShellExecuteW,
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
};

/// Pin ids are "aumid:<AUMID>" or an exe path, same as the tracker's group ids.
fn launch_target(id: &str) -> String {
    match id.strip_prefix("aumid:") {
        Some(a) => format!(r"shell:AppsFolder\{a}"),
        None => id.to_string(),
    }
}

/// Pins in effect: the dock's own (config) plus the Windows taskbar's, minus the ones removed here.
pub fn effective() -> Vec<crate::config::Pin> {
    let cfg = crate::config::Config::load(&crate::config::config_path());
    let mut pins = cfg.pins.clone();
    for p in super::taskbar_pins::read() {
        if cfg.hidden_pins.iter().any(|h| h.eq_ignore_ascii_case(&p.id)) {
            continue;
        }
        if !pins.iter().any(|x| x.id.eq_ignore_ascii_case(&p.id)) {
            pins.push(p);
        }
    }
    pins
}

/// Pin an app to the dock or take it off. Removing a pin that came from the Windows taskbar is remembered.
pub fn set_pinned(app: &tauri::AppHandle, id: &str, pinned: bool) -> Result<(), String> {
    let mut c = crate::settings::current()?;
    let same = |a: &str| a.eq_ignore_ascii_case(id);
    if pinned {
        if !c.pins.iter().any(|p| same(&p.id)) {
            c.pins.push(crate::config::Pin { id: id.to_string() });
        }
        c.hidden_pins.retain(|h| !same(h));
    } else {
        c.pins.retain(|p| !same(&p.id));
        if super::taskbar_pins::read().iter().any(|p| same(&p.id)) && !c.hidden_pins.iter().any(|h| same(h)) {
            c.hidden_pins.push(id.to_string());
        }
    }
    crate::settings::save_and_emit(app, &c)?;
    tracker::set_pins(effective());
    Ok(())
}

/// Follows the Windows taskbar's pin folder: an app pinned there (right-click > Pin to taskbar) appears on the dock,
/// even if it was removed from the dock before.
pub fn watch() {
    std::thread::spawn(|| {
        let mut last = super::taskbar_pins::signature();
        let mut known: Vec<String> = super::taskbar_pins::read().into_iter().map(|p| p.id).collect();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let now = super::taskbar_pins::signature();
            if now == last {
                continue;
            }
            last = now;
            let ids: Vec<String> = super::taskbar_pins::read().into_iter().map(|p| p.id).collect();
            let fresh: Vec<&String> = ids.iter().filter(|i| !known.iter().any(|k| k.eq_ignore_ascii_case(i))).collect();
            if !fresh.is_empty() {
                // Pinned again on the Windows side: it comes back to the dock.
                let path = crate::config::config_path();
                if let Ok(mut c) = crate::config::Config::try_load(&path) {
                    let before = c.hidden_pins.len();
                    c.hidden_pins.retain(|h| !fresh.iter().any(|f| f.eq_ignore_ascii_case(h)));
                    if c.hidden_pins.len() != before {
                        let _ = c.save(&path);
                    }
                }
            }
            known = ids;
            tracker::set_pins(effective());
            log::info!("[pins] Windows taskbar pins changed");
        }
    });
}

pub(crate) fn launch(id: &str) -> Result<(), String> {
    let target = HSTRING::from(launch_target(id));
    let r = unsafe { ShellExecuteW(Some(HWND::default()), PCWSTR::null(), &target, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    if r.0 as isize > 32 { Ok(()) } else { Err(format!("launch failed: {id}")) }
}

/// Moves a window to the primary monitor's work area (centered, size kept) when it is on another one.
/// Done through the window placement, so a maximized window lands maximized on the new monitor
/// without the restore/maximize animation.
pub(super) fn move_to_main(h: isize) {
    use windows::Win32::{Foundation::POINT, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    let hwnd = HWND(h as *mut _);
    unsafe {
        let primary = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        if MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) == primary {
            return;
        }
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let mut pl = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        if !GetMonitorInfoW(primary, &mut mi).as_bool() || GetWindowPlacement(hwnd, &mut pl).is_err() {
            return;
        }
        let wk = mi.rcWork;
        let n = pl.rcNormalPosition;
        let (w, ht) = ((n.right - n.left).min(wk.right - wk.left), (n.bottom - n.top).min(wk.bottom - wk.top));
        let (x, y) = (wk.left + (wk.right - wk.left - w) / 2, wk.top + (wk.bottom - wk.top - ht) / 2);
        pl.rcNormalPosition = windows::Win32::Foundation::RECT { left: x, top: y, right: x + w, bottom: y + ht };
        let _ = SetWindowPlacement(hwnd, &pl);
    }
}

/// Is the configured modifier held right now? ("always" and unknown values: yes.)
pub(super) fn key_held(key: &str) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    let vk = match key {
        "lctrl" => VK_LCONTROL,
        "rctrl" => VK_RCONTROL,
        "lalt" => VK_LMENU,
        "ralt" => VK_RMENU,
        "lshift" => VK_LSHIFT,
        "rshift" => VK_RSHIFT,
        "lwin" => VK_LWIN,
        "rwin" => VK_RWIN,
        _ => return true,
    };
    (unsafe { GetAsyncKeyState(vk.0 as i32) }) < 0
}

/// After a launch: waits for the app's window(s) to appear, then moves them to the primary monitor.
fn move_when_open(id: String) {
    std::thread::spawn(move || {
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if tracker::group_state(&id).is_some_and(|g| !g.windows.is_empty()) {
                std::thread::sleep(std::time::Duration::from_millis(400)); // let the window finish sizing itself
                for w in tracker::group_state(&id).map(|g| g.windows).unwrap_or_default() {
                    move_to_main(w);
                }
                return;
            }
        }
    });
}

fn click(id: &str) -> Result<(), String> {
    let cfg = crate::config::Config::load(&crate::config::config_path());
    let main = cfg.bring_to_main && key_held(&cfg.bring_key);
    match decide_click(&tracker::group_state(id).unwrap_or_default()) {
        ClickAction::Launch => {
            launch(id)?;
            if main {
                move_when_open(id.to_string());
            }
        }
        ClickAction::Focus(h) => {
            if main {
                move_to_main(h);
            }
            tracker::activate(h)
        }
        ClickAction::Minimize(h) => tracker::minimize(h),
    }
    Ok(())
}

#[tauri::command]
pub async fn app_click(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || click(&id)).await.map_err(|e| e.to_string())?
}

fn press_win() -> Result<(), String> {
    let k = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VK_LWIN, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() },
        },
    };
    let seq = [k(false), k(true)];
    let sent = unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == seq.len() { Ok(()) } else { Err("SendInput failed".into()) }
}

#[tauri::command]
pub async fn start_menu() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(press_win).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::launch_target;
    #[test]
    fn targets() {
        assert_eq!(launch_target("aumid:Foo!App"), r"shell:AppsFolder\Foo!App");
        assert_eq!(launch_target(r"C:\a\b.exe"), r"C:\a\b.exe");
    }
}
