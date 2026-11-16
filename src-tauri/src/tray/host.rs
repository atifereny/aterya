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

/// Win+B focuses the tray's "Show hidden icons" button; Enter opens the flyout.
#[tauri::command]
pub async fn open_tray_overflow() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() } },
        };
        let send = |v: &[INPUT]| unsafe { SendInput(v, size_of::<INPUT>() as i32) };
        send(&[key(VK_LWIN, false), key(VK_B, false), key(VK_B, true), key(VK_LWIN, true)]);
        std::thread::sleep(std::time::Duration::from_millis(250));
        send(&[key(VK_RETURN, false), key(VK_RETURN, true)]);
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
