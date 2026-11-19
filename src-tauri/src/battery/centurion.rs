//! PRO X 2 LIGHTSPEED headset battery over Logitech's "Centurion" protocol (vendor page 0xFFA0,
//! interface 3, 64-byte reports with report id 0x51), reached through the dongle's bridge feature.
//! Layout follows HeadsetControl's logitech_centurion_protocol.hpp.
use super::hidpp::BatteryReading;
use hidapi::{HidApi, HidDevice};

const VID: u16 = 0x046D;
const PID: u16 = 0x0AF7;
const SW_ID: u8 = 0x01;
const BATTERY_SOC: [u8; 2] = [0x01, 0x04];
const BRIDGE_FEATURE: [u8; 2] = [0x00, 0x03];
const READ_MS: i32 = 400;
const ATTEMPTS: usize = 8;

fn frame(payload: &[u8]) -> [u8; 64] {
    let mut f = [0u8; 64];
    f[0] = 0x51;
    f[1] = payload.len() as u8 + 1;
    f[3..3 + payload.len()].copy_from_slice(payload);
    f
}

/// Payload of a frame: the bytes after the length and flags bytes.
fn extract(resp: &[u8]) -> Option<&[u8]> {
    let cpl = *resp.get(1)? as usize;
    (resp.first() == Some(&0x51) && cpl > 1 && cpl + 2 <= resp.len()).then(|| &resp[3..2 + cpl])
}

fn send(dev: &HidDevice, payload: &[u8]) -> Option<()> {
    let mut buf = [0u8; 64];
    while dev.read_timeout(&mut buf, 0).ok()? > 0 {} // drop stale notifications
    dev.write(&frame(payload)).ok().map(|_| ())
}

fn next_payload(dev: &HidDevice) -> Option<Vec<u8>> {
    let mut buf = [0u8; 64];
    let n = dev.read_timeout(&mut buf, READ_MS).ok()?;
    if n == 0 {
        return None; // timeout: headset off
    }
    Some(extract(&buf[..n]).map(<[u8]>::to_vec).unwrap_or_default())
}

/// Request to a dongle feature; returns the reply bytes after [feature, function|sw].
fn direct(dev: &HidDevice, feature: u8, func: u8, params: &[u8]) -> Option<Vec<u8>> {
    let mut p = vec![feature, (func & 0xF0) | SW_ID];
    p.extend_from_slice(params);
    send(dev, &p)?;
    for _ in 0..ATTEMPTS {
        let r = next_payload(dev)?;
        if r.len() >= 2 && r[0] == feature {
            return Some(r[2..].to_vec());
        }
    }
    None
}

/// Request to a headset feature through the bridge; returns the data after the sub-header.
fn bridge(dev: &HidDevice, bridge: u8, sub: u8, func: u8, params: &[u8]) -> Option<Vec<u8>> {
    let mut msg = vec![0x00, sub, (func & 0xF0) | SW_ID];
    msg.extend_from_slice(params);
    let mut p = vec![bridge, 0x10 | SW_ID, ((msg.len() >> 8) & 0x0F) as u8, (msg.len() & 0xFF) as u8];
    p.extend_from_slice(&msg);
    send(dev, &p)?;
    for _ in 0..ATTEMPTS {
        let r = next_payload(dev)?;
        if r.len() < 2 || r[0] != bridge || r[1] >> 4 != 1 || r[1] & 0x0F != 0 {
            continue; // other feature, the ack (sw id 1), or a notification
        }
        if r.len() >= 7 && r[4] == 0 && r[5] == sub {
            return Some(r[7..].to_vec());
        }
        if r.len() >= 7 && r[5] == 0xFF {
            return None; // rejected
        }
    }
    None
}

pub fn parse_battery(data: &[u8]) -> Option<BatteryReading> {
    let level = *data.first()?;
    (level <= 100).then(|| BatteryReading { percent: level, charging: matches!(data.get(2), Some(1 | 2)) })
}

pub fn read() -> Option<BatteryReading> {
    let api = HidApi::new().ok()?;
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == VID && d.product_id() == PID && d.usage_page() == 0xFFA0 && d.usage() == 0x0001)?;
    let dev = info.open_device(&api).ok()?;
    let b = *direct(&dev, 0x00, 0x00, &BRIDGE_FEATURE)?.first().filter(|&&i| i != 0)?;
    let sub = *bridge(&dev, b, 0x00, 0x00, &BATTERY_SOC)?.first().filter(|&&i| i != 0)?;
    parse_battery(&bridge(&dev, b, sub, 0x00, &[])?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_layout() {
        let f = frame(&[0x00, 0x11, 0x00, 0x03]);
        assert_eq!(&f[..7], &[0x51, 5, 0, 0x00, 0x11, 0x00, 0x03]);
    }
    #[test]
    fn extract_payload() {
        let mut r = [0u8; 64];
        r[..6].copy_from_slice(&[0x51, 4, 0, 7, 0x11, 9]);
        assert_eq!(extract(&r), Some(&[7u8, 0x11, 9][..]));
        r[0] = 0x50;
        assert_eq!(extract(&r), None);
    }
    #[test]
    fn battery_parse() {
        assert_eq!(parse_battery(&[80, 0, 1]), Some(BatteryReading { percent: 80, charging: true }));
        assert_eq!(parse_battery(&[101]), None);
    }
    /// Live: run with the headset on. `cargo test live_headset -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_headset() {
        println!("{:?}", read());
    }
}
