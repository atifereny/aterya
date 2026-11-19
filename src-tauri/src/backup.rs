//! Settings backup: everything the user set (config.json, sticky notes) in one JSON file, exported through a
//! Save dialog and imported through an Open dialog. Screen time and usage numbers are not part of it.
use crate::config::{self, Config};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use windows::{
    Win32::{
        Foundation::HWND,
        System::SystemInformation::GetLocalTime,
        UI::Controls::Dialogs::{GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW},
    },
    core::{PCWSTR, PWSTR},
};

const FORMAT: u32 = 1;
const MAX_BYTES: u64 = 2 << 20;

/// The file dialog; None when cancelled. Runs on the calling (blocking) thread.
fn dialog(save: bool, owner: isize, name: &str) -> Option<PathBuf> {
    let mut file = [0u16; 520];
    for (i, u) in name.encode_utf16().take(260).enumerate() {
        file[i] = u;
    }
    let filter: Vec<u16> = "Aterya yedeği (*.json)\0*.json\0\0".encode_utf16().collect();
    let title: Vec<u16> = (if save { "Ayarları dışa aktar" } else { "Yedekten yükle" }).encode_utf16().chain(Some(0)).collect();
    let ext: Vec<u16> = "json\0".encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND(owner as *mut _),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        lpstrDefExt: PCWSTR(ext.as_ptr()),
        Flags: if save { OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR } else { OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR },
        ..Default::default()
    };
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } }.as_bool();
    ok.then(|| {
        let n = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        PathBuf::from(String::from_utf16_lossy(&file[..n]))
    })
}

fn settings_hwnd(app: &AppHandle) -> isize {
    app.get_webview_window("settings").and_then(|w| w.hwnd().ok()).map_or(0, |h| h.0 as isize)
}

fn bundle() -> Result<String, String> {
    let cfg: serde_json::Value = serde_json::to_value(Config::try_load(&config::config_path()).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let t = unsafe { GetLocalTime() };
    let v = serde_json::json!({
        "aterya": FORMAT,
        "exportedAt": format!("{:04}-{:02}-{:02} {:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute),
        "config": cfg,
        "notes": crate::notes::all(),
    });
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// Save the settings to a file the user picks. Returns the path, or "" when cancelled.
#[tauri::command]
pub async fn backup_export(app: AppHandle) -> Result<String, String> {
    let owner = settings_hwnd(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let t = unsafe { GetLocalTime() };
        let name = format!("aterya-yedek-{:04}-{:02}-{:02}.json", t.wYear, t.wMonth, t.wDay);
        let Some(path) = dialog(true, owner, &name) else { return Ok(String::new()) };
        std::fs::write(&path, bundle()?).map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Load settings from a backup the user picks. Returns a short message, or "" when cancelled.
#[tauri::command]
pub async fn backup_import(app: AppHandle) -> Result<String, String> {
    let owner = settings_hwnd(&app);
    let picked = tauri::async_runtime::spawn_blocking(move || dialog(false, owner, "")).await.map_err(|e| e.to_string())?;
    let Some(path) = picked else { return Ok(String::new()) };
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("Dosya çok büyük, bir Aterya yedeği değil gibi görünüyor.".into());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    apply(&app, &text)
}

fn apply(app: &AppHandle, text: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|_| "Bu dosya bir Aterya yedeği değil (JSON okunamadı).".to_string())?;
    if v.get("aterya").and_then(|f| f.as_u64()) != Some(FORMAT as u64) {
        return Err("Bu dosya bir Aterya yedeği değil.".into());
    }
    let mut cfg: Config = serde_json::from_value(v.get("config").cloned().unwrap_or_default()).map_err(|e| format!("Yedekteki ayarlar okunamadı: {e}"))?;
    let path = config::config_path();
    let current = Config::try_load(&path).map_err(|e| e.to_string())?; // a broken current file is never overwritten
    cfg.layout = cfg.layout.sanitized();
    cfg.theme = cfg.theme.sanitized();
    cfg.autostart = current.autostart; // this PC's start-with-Windows choice stays
    cfg.dock_text = crate::settings::valid_color(&cfg.dock_text);
    cfg.dock_accent = crate::settings::valid_color(&cfg.dock_accent);
    let _ = std::fs::copy(&path, path.with_file_name("config.before-import.json")); // one step back
    cfg.save(&path).map_err(|e| e.to_string())?;
    let notes = v.get("notes").and_then(|n| serde_json::from_value::<Vec<crate::notes::Note>>(n.clone()).ok());
    let note_count = notes.as_ref().map_or(0, |n| n.len());
    if let Some(n) = notes {
        crate::notes::replace(n);
    }
    crate::settings::save_and_emit(app, &cfg)?;
    crate::shell::apply_dock(app);
    // Desktop widgets and notes come back at their saved places.
    for (label, w) in app.webview_windows() {
        if label.starts_with("float-") || label.starts_with("note-") {
            let _ = w.close();
        }
    }
    let app2 = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500)); // let the old windows finish closing
        crate::float::restore(&app2);
        crate::notes::restore(&app2);
    });
    Ok(format!("Yedek yüklendi ({} not). Önceki ayarların config.before-import.json olarak saklandı.", note_count))
}
