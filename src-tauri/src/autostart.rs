//! Start with Windows: an HKCU Run entry pointing at the release exe. Never registered from a dev build.
use windows::core::{w, HSTRING};
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

const KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const NAME: windows::core::PCWSTR = w!("custom_taskbar");

pub fn enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, NAME, RRF_RT_REG_SZ, None, None, None) }.is_ok()
}

pub fn set(on: bool) -> Result<(), String> {
    if !on {
        let _ = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, KEY, NAME) };
        return Ok(());
    }
    if cfg!(debug_assertions) {
        return Err("Geliştirme sürümü başlangıca eklenmez; release sürümünde çalışır.".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let value = HSTRING::from(format!("\"{}\"", exe.display()));
    let bytes = (value.len() + 1) * 2; // include the terminating NUL
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, NAME, REG_SZ.0, Some(value.as_ptr() as *const _), bytes as u32) }
        .ok()
        .map_err(|e| e.to_string())
}

/// First run registers the entry; afterwards the user's choice (kept in config.json) decides.
pub fn ensure() {
    if cfg!(debug_assertions) {
        return;
    }
    let cfg = crate::config::Config::load(&crate::config::config_path());
    match cfg.autostart {
        Some(_) => {} // the user's choice stands, even if they removed the entry in Task Manager
        None => {
            if !enabled() && set(true).is_ok() {
                log::info!("autostart registered");
            }
        }
    }
}
