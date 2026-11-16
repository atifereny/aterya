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

fn launch(id: &str) -> Result<(), String> {
    let target = HSTRING::from(launch_target(id));
    let r = unsafe { ShellExecuteW(Some(HWND::default()), PCWSTR::null(), &target, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    if r.0 as isize > 32 { Ok(()) } else { Err(format!("launch failed: {id}")) }
}

fn click(id: &str) -> Result<(), String> {
    match decide_click(&tracker::group_state(id).unwrap_or_default()) {
        ClickAction::Launch => launch(id)?,
        ClickAction::Focus(h) => tracker::activate(h),
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
