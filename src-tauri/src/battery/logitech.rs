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
    /// Last known value shown while the device sleeps (keyboard only); `percent` holds that value.
    pub asleep: bool,
}

const VID: u16 = 0x046D;
const TIMEOUT: Duration = Duration::from_secs(2);

// (id, name, reader). Swap a reader to change how a device is read.
const DEVICES: [(&str, &str, fn() -> Option<BatteryReading>); 3] = [
    ("mouse", "G502", read_mouse),
    ("headset", "PRO X 2", read_headset),
    ("keyboard", "Neo87", super::neo87::read),
];

fn read_mouse() -> Option<BatteryReading> {
    read_hidpp(0xC547, 0x01)
}

// 0AF7 has no HID++ collection; it speaks Centurion on page 0xFFA0.
fn read_headset() -> Option<BatteryReading> {
    super::centurion::read()
}

/// HID++ 1.0 short error report (0x10, 0x8F) echoing our device index, feature and function byte.
fn is_short_error_for(r: &[u8], idx: u8, feat: u8, func_sw: u8) -> bool {
    r.len() >= 5 && r[0] == 0x10 && r[1] == idx && r[2] == 0x8F && r[3] == feat && r[4] == func_sw
}

/// Writes a request and waits for the matching reply; skips unrelated notifications (G HUB shares the pipe).
/// Returns None on timeout or a 1.0 short error (device offline), so callers fail fast.
fn call(dev: &HidDevice, idx: u8, feat: u8, func: u8, params: &[u8]) -> Option<Vec<u8>> {
    dev.write(&hidpp::request(idx, feat, func, params)).ok()?;
    let fs = (func << 4) | 0x0A;
    let end = Instant::now() + TIMEOUT;
    let mut buf = [0u8; 64];
    loop {
        let left = end.checked_duration_since(Instant::now())?;
        let n = dev.read_timeout(&mut buf, left.as_millis() as i32).ok()?;
        if n == 0 {
            return None;
        }
        let r = &buf[..n];
        if is_short_error_for(r, idx, feat, fs) {
            return None;
        }
        if r.len() < 7 || r[0] != 0x11 || r[1] != idx {
            continue;
        }
        // 2.0 error reports echo the failed feature/function at bytes 3..5.
        if (r[2] == 0xFF && r[3] == feat && r[4] == fs) || (r[2] == feat && r[3] == fs) {
            return Some(r.to_vec());
        }
    }
}

#[derive(Debug, PartialEq)]
enum Lookup {
    Found(u8),
    Unsupported, // ROOT answered: feature index 0
    NoAnswer,    // timeout / error: device offline, stop probing
}

fn classify_root(resp: Option<&[u8]>) -> Lookup {
    match resp {
        None => Lookup::NoAnswer,
        Some(r) if hidpp::is_error(r) => Lookup::NoAnswer,
        Some(r) => hidpp::parse_root_get_feature(r).map_or(Lookup::Unsupported, Lookup::Found),
    }
}

fn feature_index(dev: &HidDevice, idx: u8, feature: u16) -> Lookup {
    classify_root(call(dev, idx, 0x00, 0, &feature.to_be_bytes()).as_deref())
}

/// Outer None = no answer (try another collection); Some(x) = definitive.
fn read_battery(dev: &HidDevice, idx: u8) -> Option<Option<BatteryReading>> {
    type Parse = fn(&[u8]) -> Option<BatteryReading>;
    let tries: [(u16, u8, Parse); 3] = [
        (0x1004, 1, hidpp::parse_unified_battery),
        (0x1001, 0, hidpp::parse_battery_voltage),
        (0x1000, 0, hidpp::parse_battery_status),
    ];
    for (feature, func, parse) in tries {
        match feature_index(dev, idx, feature) {
            Lookup::Found(f) => return Some(call(dev, idx, f, func, &[]).and_then(|r| parse(&r))),
            Lookup::Unsupported => continue,
            Lookup::NoAnswer => return None,
        }
    }
    Some(None)
}

/// Opens the vendor collections (usage page 0xFF00) of a pid, long-report collection first.
fn read_hidpp(pid: u16, idx: u8) -> Option<BatteryReading> {
    let api = HidApi::new().ok()?; // fresh enumeration each poll handles hot-plug
    let mut infos: Vec<_> = api
        .device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == pid && d.usage_page() == 0xFF00)
        .collect();
    infos.sort_by_key(|d| d.usage() != 0x0002);
    infos
        .iter()
        .find_map(|i| read_battery(&i.open_device(&api).ok()?, idx))
        .flatten()
}

/// One reading per device. A device that answers nothing but has answered before is reported
/// `asleep` with its last value (keyboards doze off; the dongle stays silent until a key press).
fn snapshot(last: &mut [Option<BatteryReading>; 3], due: [bool; 3]) -> Vec<DeviceBattery> {
    DEVICES
        .iter()
        .enumerate()
        .map(|(i, &(id, name, reader))| {
            if due[i] {
                let r = reader();
                if r.is_some() || id != "keyboard" {
                    last[i] = r;
                }
                if r.is_none() {
                    return DeviceBattery { id, name, percent: last[i].map(|l| l.percent), charging: false, connected: last[i].is_some(), asleep: last[i].is_some() };
                }
            }
            let r = last[i];
            DeviceBattery { id, name, percent: r.map(|r| r.percent), charging: r.is_some_and(|r| r.charging), connected: r.is_some(), asleep: false }
        })
        .collect()
}

/// Mouse and headset are read every `poll_minutes`; the keyboard every KEYBOARD_POLL, so its wake-up
/// shows quickly (and the poll itself may keep the dongle link busy). Emits only on change.
pub fn start(app: AppHandle, poll_minutes: u64) {
    const KEYBOARD_POLL: Duration = Duration::from_secs(15);
    std::thread::spawn(move || {
        let mut last = [None; 3];
        let mut prev: Option<Vec<DeviceBattery>> = None;
        let mut slow_at = Instant::now();
        let mut first = true;
        loop {
            let slow = first || slow_at.elapsed() >= Duration::from_secs(poll_minutes.max(1) * 60);
            if slow {
                slow_at = Instant::now();
            }
            first = false;
            let now = snapshot(&mut last, [slow, slow, true]);
            if prev.as_ref() != Some(&now) {
                app.state::<Hub>().emit("battery://changed", now.clone());
                prev = Some(now);
            }
            std::thread::sleep(KEYBOARD_POLL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(idx: u8) -> Vec<u8> {
        let mut r = vec![0x11, 1, 0x00, 0x0A, idx];
        r.resize(20, 0);
        r
    }

    #[test]
    fn classify_root_three_way() {
        assert_eq!(classify_root(Some(&root(7))), Lookup::Found(7));
        assert_eq!(classify_root(Some(&root(0))), Lookup::Unsupported);
        assert_eq!(classify_root(None), Lookup::NoAnswer);
        let mut e = root(0);
        e[2] = 0xFF;
        assert_eq!(classify_root(Some(&e)), Lookup::NoAnswer);
    }

    #[test]
    fn short_error_matcher() {
        let e = [0x10, 1, 0x8F, 0x00, 0x0A, 0x09, 0x00];
        assert!(is_short_error_for(&e, 1, 0x00, 0x0A));
        assert!(!is_short_error_for(&e, 2, 0x00, 0x0A));
        assert!(!is_short_error_for(&e, 1, 0x05, 0x0A));
        assert!(!is_short_error_for(&[0x11, 1, 0x8F, 0, 0x0A, 0, 0], 1, 0, 0x0A));
    }

    /// Real hardware, read-only: `cargo test probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn probe() {
        for d in super::snapshot(&mut [None; 3], [true; 3]) {
            println!("{d:?}");
        }
    }
}
