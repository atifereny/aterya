//! Volume (Core Audio), network (NLM + WinRT) and Win+A / Win+N shortcuts.
use crate::state::Hub;
use serde::Serialize;
use std::sync::mpsc::{Sender, channel};
use tauri::{AppHandle, Manager};
use windows::{
    Networking::Connectivity::{NetworkInformation, NetworkStatusChangedEventHandler},
    Win32::{
        Foundation::PROPERTYKEY,
        Media::Audio::{
            AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole,
            Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl},
            IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, eConsole,
            eRender,
        },
        Networking::NetworkListManager::{
            INetworkListManager, NLM_CONNECTIVITY_IPV4_INTERNET, NLM_CONNECTIVITY_IPV6_INTERNET, NetworkListManager,
        },
        System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
        UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_A, VK_LWIN, VK_N,
        },
    },
    core::{PCWSTR, implement},
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Ethernet,
    Wifi,
    Offline,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemState {
    pub volume: u8,
    pub muted: bool,
    pub network: Network,
}

fn scalar_to_percent(s: f32) -> u8 {
    (s.clamp(0.0, 1.0) * 100.0).round() as u8
}

fn percent_to_scalar(p: u8) -> f32 {
    f32::from(p.min(100)) / 100.0
}

fn classify_network(connected: bool, is_wlan: bool) -> Network {
    match (connected, is_wlan) {
        (false, _) => Network::Offline,
        (true, true) => Network::Wifi,
        (true, false) => Network::Ethernet,
    }
}

#[derive(Debug, Clone, Copy)]
enum Sig {
    Volume,
    Device,
    Network,
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolCb(Sender<Sig>);

impl IAudioEndpointVolumeCallback_Impl for VolCb_Impl {
    fn OnNotify(&self, _: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        let _ = self.0.send(Sig::Volume);
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DevCb(Sender<Sig>);

impl IMMNotificationClient_Impl for DevCb_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> windows::core::Result<()> {
        if flow == eRender && role == eConsole {
            let _ = self.0.send(Sig::Device);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

fn default_endpoint(en: &IMMDeviceEnumerator) -> windows::core::Result<IAudioEndpointVolume> {
    unsafe { en.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
}

fn read_volume(ep: &IAudioEndpointVolume) -> (u8, bool) {
    unsafe {
        let v = ep.GetMasterVolumeLevelScalar().map(scalar_to_percent).unwrap_or(0);
        let m = ep.GetMute().map(|b| b.as_bool()).unwrap_or(false);
        (v, m)
    }
}

fn read_network() -> Network {
    let connected = unsafe {
        CoCreateInstance::<_, INetworkListManager>(&NetworkListManager, None, CLSCTX_ALL)
            .and_then(|m| m.GetConnectivity())
            .map(|c| c.0 & (NLM_CONNECTIVITY_IPV4_INTERNET.0 | NLM_CONNECTIVITY_IPV6_INTERNET.0) != 0)
            .unwrap_or(false)
    };
    let wlan = NetworkInformation::GetInternetConnectionProfile()
        .and_then(|p| p.IsWlanConnectionProfile())
        .unwrap_or(false);
    classify_network(connected, wlan)
}

/// Owns the endpoint and its volume callback; dropping unregisters the callback.
struct Bound {
    ep: IAudioEndpointVolume,
    cb: IAudioEndpointVolumeCallback,
}

impl Bound {
    fn new(en: &IMMDeviceEnumerator, tx: &Sender<Sig>) -> Option<Self> {
        let ep = default_endpoint(en).ok()?;
        let cb: IAudioEndpointVolumeCallback = VolCb(tx.clone()).into();
        unsafe { ep.RegisterControlChangeNotify(&cb).ok()? };
        Some(Self { ep, cb })
    }
}

impl Drop for Bound {
    fn drop(&mut self) {
        let _ = unsafe { self.ep.UnregisterControlChangeNotify(&self.cb) };
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let en: IMMDeviceEnumerator = match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(e) => e,
            Err(e) => {
                log::warn!("audio enumerator unavailable: {e}");
                return;
            }
        };
        let (tx, rx) = channel::<Sig>();
        let dev_cb: IMMNotificationClient = DevCb(tx.clone()).into();
        let _ = unsafe { en.RegisterEndpointNotificationCallback(&dev_cb) };
        let t = tx.clone();
        let _net = NetworkInformation::NetworkStatusChanged(&NetworkStatusChangedEventHandler::new(move |_| {
            let _ = t.send(Sig::Network);
            Ok(())
        }));
        let mut bound = Bound::new(&en, &tx);
        let mut last: Option<SystemState> = None;
        let emit = |bound: &Option<Bound>, last: &mut Option<SystemState>| {
            let (volume, muted) = bound.as_ref().map(|b| read_volume(&b.ep)).unwrap_or((0, false));
            let s = SystemState { volume, muted, network: read_network() };
            if last.as_ref() != Some(&s) {
                app.state::<Hub>().emit("system://changed", s.clone());
                *last = Some(s);
            }
        };
        emit(&bound, &mut last);
        while let Ok(first) = rx.recv() {
            let mut rebind = matches!(first, Sig::Device);
            rebind |= rx.try_iter().fold(false, |a, s| a | matches!(s, Sig::Device));
            if rebind {
                drop(bound.take()); // unregisters the old callback before binding the new endpoint
                bound = Bound::new(&en, &tx);
            }
            emit(&bound, &mut last);
        }
    });
}

/// Runs on a blocking-pool thread: its own COM init and its own endpoint.
async fn with_endpoint<T: Send + 'static>(
    f: impl FnOnce(&IAudioEndpointVolume) -> windows::core::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let en: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(|e| e.to_string())?;
        let ep = default_endpoint(&en).map_err(|e| e.to_string())?;
        f(&ep).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_volume(value: u8) -> Result<(), String> {
    with_endpoint(move |ep| unsafe { ep.SetMasterVolumeLevelScalar(percent_to_scalar(value), std::ptr::null()) }).await
}

#[tauri::command]
pub async fn toggle_mute() -> Result<(), String> {
    with_endpoint(|ep| unsafe { ep.SetMute(!ep.GetMute()?.as_bool(), std::ptr::null()) }).await
}

fn send_win_combo(key: VIRTUAL_KEY) -> Result<(), String> {
    let k = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() },
        },
    };
    let seq = [k(VK_LWIN, false), k(key, false), k(key, true), k(VK_LWIN, true)];
    let sent = unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == seq.len() { Ok(()) } else { Err("SendInput failed".into()) }
}

async fn shortcut(key: VIRTUAL_KEY) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || send_win_combo(key)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_quick_settings() -> Result<(), String> {
    shortcut(VK_A).await
}

#[tauri::command]
pub async fn open_notifications() -> Result<(), String> {
    shortcut(VK_N).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_rounding() {
        assert_eq!(scalar_to_percent(0.0), 0);
        assert_eq!(scalar_to_percent(0.555), 56);
        assert_eq!(scalar_to_percent(1.5), 100);
        assert_eq!(scalar_to_percent(-1.0), 0);
        assert_eq!(percent_to_scalar(50), 0.5);
        assert_eq!(percent_to_scalar(200), 1.0);
    }
    #[test]
    fn network_class() {
        assert_eq!(classify_network(false, true), Network::Offline);
        assert_eq!(classify_network(true, true), Network::Wifi);
        assert_eq!(classify_network(true, false), Network::Ethernet);
    }
}
