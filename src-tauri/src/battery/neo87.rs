//! NEO87 keyboard battery over its 2.4G dongle (0C45:FEFE, raw HID 0xFF60/0x61). See docs/superpowers/research/neo87-battery.md.
use super::hidpp::BatteryReading;
use hidapi::HidApi;

const REPLY_TIMEOUT_MS: i32 = 1000;

/// 33-byte output report: report id 0, cmd 0x20, sub 0x01, checksum (sum of 0..31) at index 32.
fn request() -> [u8; 33] {
    let mut b = [0u8; 33];
    b[1] = 0x20;
    b[2] = 0x01;
    b[32] = b[..32].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    b
}

/// Reply as hidapi returns it (report id 0 stripped): payload 20 01 00 PCT. Foreign reports are rejected.
fn parse_reply(r: &[u8]) -> Option<BatteryReading> {
    (r.len() >= 4 && r[..3] == [0x20, 0x01, 0x00]).then(|| BatteryReading { percent: r[3].min(100), charging: false })
}

/// None when dongle absent or keyboard asleep (no reply).
pub fn read() -> Option<BatteryReading> {
    let api = HidApi::new().ok()?;
    let info = api.device_list().find(|d| {
        d.vendor_id() == 0x0C45 && d.product_id() == 0xFEFE && d.usage_page() == 0xFF60 && d.usage() == 0x61
    })?;
    let dev = info.open_device(&api).ok()?;
    dev.write(&request()).ok()?;
    let end = std::time::Instant::now() + std::time::Duration::from_millis(REPLY_TIMEOUT_MS as u64);
    let mut buf = [0u8; 64];
    while let Some(left) = end.checked_duration_since(std::time::Instant::now()) {
        let n = dev.read_timeout(&mut buf, left.as_millis() as i32).ok()?;
        if n == 0 {
            return None;
        }
        if let Some(r) = parse_reply(&buf[..n]) {
            return Some(r);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout() {
        let r = request();
        assert_eq!(&r[..4], &[0, 0x20, 0x01, 0]);
        assert_eq!(r[32], 0x21);
    }

    #[test]
    fn parses_reply() {
        let mut f = vec![0x20, 0x01, 0x00, 0x45];
        f.resize(32, 0);
        assert_eq!(parse_reply(&f), Some(BatteryReading { percent: 69, charging: false }));
        f[3] = 200;
        assert_eq!(parse_reply(&f).unwrap().percent, 100);
        f[2] = 1;
        assert_eq!(parse_reply(&f), None);
    }

    /// Real hardware, read-only: `cargo test neo87_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn neo87_probe() {
        println!("{:?}", read());
    }
}
