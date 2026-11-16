//! First-run autostart: HKCU Run entry pointing at the release exe. Never from a dev build.
use windows::core::{w, HSTRING};
use windows::Win32::System::Registry::{RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

pub fn ensure() {
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let key = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = w!("custom_taskbar");
    // Only when missing: a user who removed or repointed the entry keeps their choice.
    let exists = unsafe { RegGetValueW(HKEY_CURRENT_USER, key, name, RRF_RT_REG_SZ, None, None, None) }.is_ok();
    if exists {
        return;
    }
    let value = HSTRING::from(format!("\"{}\"", exe.display()));
    let bytes = (value.len() + 1) * 2; // include the terminating NUL
    let r = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_SZ.0, Some(value.as_ptr() as *const _), bytes as u32) };
    if r.is_err() {
        log::warn!("autostart registration failed: {r:?}");
    } else {
        log::info!("autostart registered for {}", exe.display());
    }
}
