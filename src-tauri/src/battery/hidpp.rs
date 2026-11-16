//! HID++ 2.0 long-report framing and battery response parsing (pure, no I/O).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryReading {
    pub percent: u8,
    pub charging: bool,
}

const LONG_ID: u8 = 0x11;
const SW_ID: u8 = 0x0A;

/// Builds a 20-byte HID++ long request.
pub fn request(device_index: u8, feature_index: u8, function: u8, params: &[u8]) -> [u8; 20] {
    let mut r = [0u8; 20];
    r[0] = LONG_ID;
    r[1] = device_index;
    r[2] = feature_index;
    r[3] = (function << 4) | SW_ID;
    for (d, s) in r[4..].iter_mut().zip(params) {
        *d = *s;
    }
    r
}

/// HID++ 2.0 error reports carry 0xFF in the feature byte (1.0 uses 0x8F).
pub fn is_error(resp: &[u8]) -> bool {
    resp.len() < 7 || resp[0] != LONG_ID || resp[2] == 0xFF || resp[2] == 0x8F
}

/// ROOT.GetFeature (0x0000, function 0) reply: byte 4 is the feature index (0 = unsupported).
pub fn parse_root_get_feature(resp: &[u8]) -> Option<u8> {
    if is_error(resp) || resp[4] == 0 { None } else { Some(resp[4]) }
}

/// Unified battery 0x1004, function 1 (GetStatus): [percent, level, status, ..].
pub fn parse_unified_battery(resp: &[u8]) -> Option<BatteryReading> {
    if is_error(resp) {
        return None;
    }
    Some(BatteryReading { percent: resp[4].min(100), charging: (1..=3).contains(&resp[6]) })
}

/// Battery status 0x1000, function 0: [percent, next_level, status, ..].
pub fn parse_battery_status(resp: &[u8]) -> Option<BatteryReading> {
    if is_error(resp) {
        return None;
    }
    Some(BatteryReading { percent: resp[4].min(100), charging: (1..=4).contains(&resp[6]) })
}

// (mV, %) single-cell Li-ion discharge curve, descending.
const LIION: [(u16, u8); 11] = [
    (4186, 100), (4067, 90), (3989, 80), (3922, 70), (3859, 60), (3811, 50),
    (3778, 40), (3750, 30), (3710, 20), (3647, 10), (3300, 0),
];

fn mv_to_percent(mv: u16) -> u8 {
    if mv >= LIION[0].0 {
        return 100;
    }
    for w in LIION.windows(2) {
        let ((hi, hp), (lo, lp)) = (w[0], w[1]);
        if mv >= lo {
            return lp + ((mv - lo) as u32 * (hp - lp) as u32 / (hi - lo) as u32) as u8;
        }
    }
    0
}

/// Battery voltage 0x1001, function 0: [mV hi, mV lo, flags]; flags bit 7 = charging.
pub fn parse_battery_voltage(resp: &[u8]) -> Option<BatteryReading> {
    if is_error(resp) {
        return None;
    }
    let mv = u16::from_be_bytes([resp[4], resp[5]]);
    Some(BatteryReading { percent: mv_to_percent(mv), charging: resp[6] & 0x80 != 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(feature: u8, params: &[u8]) -> Vec<u8> {
        let mut r = vec![0x11, 0x01, feature, 0x1A];
        r.extend_from_slice(params);
        r.resize(20, 0);
        r
    }

    #[test]
    fn request_layout() {
        let r = request(0x01, 0x05, 0x01, &[]);
        assert_eq!(&r[..4], &[0x11, 0x01, 0x05, 0x1A]);
        assert!(r[4..].iter().all(|b| *b == 0));
    }

    #[test]
    fn unified_battery() {
        let r = parse_unified_battery(&resp(0x05, &[0x55, 0x08, 0x01, 0x01])).unwrap();
        assert_eq!(r, BatteryReading { percent: 85, charging: true });
        let r = parse_unified_battery(&resp(0x05, &[0x55, 0x08, 0x00, 0x01])).unwrap();
        assert!(!r.charging);
    }

    #[test]
    fn voltage_table() {
        assert!(parse_battery_voltage(&resp(0x05, &[0x10, 0x04, 0x00])).unwrap().percent >= 90); // 4100
        assert!(parse_battery_voltage(&resp(0x05, &[0x0D, 0xAC, 0x00])).unwrap().percent <= 10); // 3500
        assert!(parse_battery_voltage(&resp(0x05, &[0x10, 0x04, 0x80])).unwrap().charging);
    }

    #[test]
    fn parse_error_response_is_disconnected() {
        let e = resp(0xFF, &[0x00, 0x00, 0x00]);
        assert_eq!(parse_unified_battery(&e), None);
        assert_eq!(parse_battery_voltage(&e), None);
        assert_eq!(parse_root_get_feature(&e), None);
    }

    #[test]
    fn root_feature_index() {
        assert_eq!(parse_root_get_feature(&resp(0x00, &[0x07])), Some(7));
        assert_eq!(parse_root_get_feature(&resp(0x00, &[0x00])), None);
    }
}
