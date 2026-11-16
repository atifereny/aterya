//! Logitech batteries over HID++ 2.0 (G502 behind LIGHTSPEED receiver, PRO X 2 dongle).
use super::hidpp::{self, BatteryReading};
use crate::state::Hub;
use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceBattery {
    pub id: &'static str,
    pub name: &'static str,
    pub percent: Option<u8>,
    pub charging: bool,
    pub connected: bool,
}

const VID: u16 = 0x046D;
// (id, name, pid, HID++ device index)
const DEVICES: [(&str, &str, u16, u8); 2] = [
    ("mouse", "G502", 0xC547, 0x01),
    ("headset", "PRO X 2", 0x0AF7, 0xFF),
];
const TIMEOUT: Duration = Duration::from_secs(2);

/// Writes a request and waits for the matching reply; skips unrelated notifications (G HUB shares the pipe).
fn call(dev: &HidDevice, idx: u8, feat: u8, func: u8, params: &[u8]) -> Option<Vec<u8>> {
    dev.write(&hidpp::request(idx, feat, func, params)).ok()?;
    let end = Instant::now() + TIMEOUT;
    let mut buf = [0u8; 64];
    loop {
        let left = end.checked_duration_since(Instant::now())?;
        let n = dev.read_timeout(&mut buf, left.as_millis() as i32).ok()?;
        if n == 0 {
            return None;
        }
        let r = &buf[..n];
        if r.len() < 7 || r[0] != 0x11 || r[1] != idx {
            continue;
        }
        // Error reports echo the failed feature/function at bytes 3..5.
        if r[2] == 0xFF && r[3] == feat && r[4] == ((func << 4) | 0x0A) {
            return Some(r.to_vec());
        }
        if r[2] == feat && r[3] == ((func << 4) | 0x0A) {
            return Some(r.to_vec());
        }
    }
}

fn feature_index(dev: &HidDevice, idx: u8, feature: u16) -> Option<u8> {
    let r = call(dev, idx, 0x00, 0, &feature.to_be_bytes())?;
    hidpp::parse_root_get_feature(&r)
}

pub fn read_battery(dev: &HidDevice, idx: u8) -> Option<BatteryReading> {
    if let Some(f) = feature_index(dev, idx, 0x1004) {
        return hidpp::parse_unified_battery(&call(dev, idx, f, 1, &[])?);
    }
    if let Some(f) = feature_index(dev, idx, 0x1001) {
        return hidpp::parse_battery_voltage(&call(dev, idx, f, 0, &[])?);
    }
    let f = feature_index(dev, idx, 0x1000)?;
    hidpp::parse_battery_status(&call(dev, idx, f, 0, &[])?)
}

/// Opens the vendor collections (usage page 0xFF00) of a pid, long-report collection first.
fn try_pid(api: &HidApi, pid: u16, idx: u8) -> Option<BatteryReading> {
    let mut infos: Vec<_> = api
        .device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == pid && d.usage_page() == 0xFF00)
        .collect();
    infos.sort_by_key(|d| d.usage() != 0x0002);
    infos.iter().find_map(|i| read_battery(&i.open_device(api).ok()?, idx))
}

pub fn read_all() -> Vec<DeviceBattery> {
    let api = HidApi::new().ok(); // fresh enumeration each poll handles hot-plug
    DEVICES
        .iter()
        .map(|&(id, name, pid, idx)| {
            let r = api.as_ref().and_then(|a| try_pid(a, pid, idx));
            DeviceBattery {
                id,
                name,
                percent: r.map(|r| r.percent),
                charging: r.is_some_and(|r| r.charging),
                connected: r.is_some(),
            }
        })
        .collect()
}

pub fn start(app: AppHandle, poll_minutes: u64) {
    std::thread::spawn(move || loop {
        app.state::<Hub>().emit("battery://changed", read_all());
        std::thread::sleep(Duration::from_secs(poll_minutes.max(1) * 60));
    });
}

#[cfg(test)]
mod tests {
    /// Real hardware, read-only: `cargo test probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn probe() {
        for d in super::read_all() {
            println!("{d:?}");
        }
    }
}
