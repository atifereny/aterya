//! Microphone mute switch (default capture device) and the Recycle Bin state.
use serde::Serialize;
use windows::Win32::{
    Media::Audio::{eCapture, eConsole, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator},
    System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
    UI::Shell::{SHEmptyRecycleBinW, SHQUERYRBINFO, SHQueryRecycleBinW},
};

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mic {
    pub available: bool,
    pub muted: bool,
}

fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        en.GetDefaultAudioEndpoint(eCapture, eConsole)?.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
    }
}

fn read() -> Mic {
    match endpoint().and_then(|e| unsafe { e.GetMute() }) {
        Ok(m) => Mic { available: true, muted: m.as_bool() },
        Err(_) => Mic { available: false, muted: false },
    }
}

#[tauri::command]
pub async fn mic_get() -> Mic {
    tauri::async_runtime::spawn_blocking(read).await.unwrap_or(Mic { available: false, muted: false })
}

/// Flip the mute state of the default microphone and return the new state.
#[tauri::command]
pub async fn mic_toggle() -> Mic {
    tauri::async_runtime::spawn_blocking(|| {
        if let Ok(e) = endpoint() {
            if let Ok(m) = unsafe { e.GetMute() } {
                let _ = unsafe { e.SetMute(!m.as_bool(), std::ptr::null()) };
            }
        }
        read()
    })
    .await
    .unwrap_or(Mic { available: false, muted: false })
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bin {
    pub items: i64,
    pub bytes: i64,
}

#[tauri::command]
pub async fn bin_get() -> Bin {
    tauri::async_runtime::spawn_blocking(|| {
        let mut info = SHQUERYRBINFO { cbSize: size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
        match unsafe { SHQueryRecycleBinW(windows::core::PCWSTR::null(), &mut info) } {
            Ok(()) => Bin { items: info.i64NumItems, bytes: info.i64Size },
            Err(_) => Bin { items: 0, bytes: 0 },
        }
    })
    .await
    .unwrap_or(Bin { items: 0, bytes: 0 })
}

#[tauri::command]
pub async fn bin_open() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer.exe")
        .arg("shell:RecycleBinFolder")
        .creation_flags(0x0800_0000)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Empty the Recycle Bin; Windows asks for confirmation itself.
#[tauri::command]
pub async fn bin_empty() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| unsafe { SHEmptyRecycleBinW(None, windows::core::PCWSTR::null(), 0) }.map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}
