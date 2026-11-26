use serde::{Deserialize, Serialize};
use std::{io, path::{Path, PathBuf}};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    /// Exe path or `aumid:<AUMID>`.
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WeatherCfg {
    pub city: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

impl Default for WeatherCfg {
    fn default() -> Self {
        Self { city: "İstanbul".into(), lat: None, lon: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PollCfg {
    pub battery: u64,
    pub weather: u64,
    pub claude: u64,
}

impl Default for PollCfg {
    fn default() -> Self {
        Self { battery: 5, weather: 15, claude: 5 }
    }
}

/// Widgets the dock can show, in default order. Start button and app row are fixed.
pub const WIDGETS: [&str; 27] = ["apps", "nowplaying", "weather", "claude", "batteries", "tray", "system", "clock", "screentime", "topcpu", "topram", "brightness", "notifications", "showdesktop", "batteryring", "cpuring", "gpuring", "memory", "storage", "netspeed", "keyboardlang", "microphone", "recyclebin", "stickynotes", "clipboard", "bluetooth", "calendar"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LayoutCfg {
    pub order: Vec<String>,
    pub hidden: Vec<String>,
    /// Widgets followed by a divider line on the dock (the user places them; none by default).
    pub dividers: Vec<String>,
}

impl Default for LayoutCfg {
    fn default() -> Self {
        Self { order: WIDGETS.iter().map(|s| s.to_string()).collect(), hidden: vec![], dividers: vec![] }
    }
}

impl LayoutCfg {
    /// Known ids only, no duplicates; widgets missing from `order` are appended in default order.
    pub fn sanitized(mut self) -> Self {
        let mut seen = Vec::new();
        self.order.retain(|w| WIDGETS.contains(&w.as_str()) && !seen.contains(w) && { seen.push(w.clone()); true });
        let had_any = !self.order.is_empty();
        for w in WIDGETS {
            if !self.order.iter().any(|o| o == w) {
                // "apps" (Start + app icons) used to be fixed at the left: new configs keep it there.
                let at = if w == "apps" { 0 } else { self.order.len() };
                self.order.insert(at, w.to_string());
                // A widget added in a later version stays off until the user turns it on.
                if had_any && w != "apps" && !self.hidden.iter().any(|h| h == w) {
                    self.hidden.push(w.to_string());
                }
            }
        }
        self.hidden.retain(|w| WIDGETS.contains(&w.as_str()));
        self.hidden.dedup();
        self.dividers.retain(|w| WIDGETS.contains(&w.as_str()));
        self.dividers.dedup();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThemeCfg {
    /// "glass" | "dark" | "light"
    pub preset: String,
    /// Pill fill strength, 0.0..=0.7.
    pub opacity: f64,
}

impl Default for ThemeCfg {
    fn default() -> Self {
        Self { preset: "glass".into(), opacity: 0.14 }
    }
}

impl ThemeCfg {
    pub fn sanitized(mut self) -> Self {
        if !["glass", "dark", "light"].contains(&self.preset.as_str()) {
            self.preset = "glass".into();
        }
        self.opacity = if self.opacity.is_finite() { self.opacity.clamp(0.0, 0.7) } else { 0.14 };
        self
    }
}

pub const BRING_KEYS: [&str; 9] = ["always", "lctrl", "rctrl", "lalt", "ralt", "lshift", "rshift", "lwin", "rwin"];

/// A widget living in its own window instead of the dock. x/y are physical screen pixels.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FloatCfg {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub pinned: bool,
}

/// What the user chose for one Bluetooth device: shown on the dock or not, and its icon key (empty: guessed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BtPref {
    pub show: bool,
    pub icon: String,
}

impl Default for BtPref {
    fn default() -> Self {
        Self { show: true, icon: String::new() }
    }
}

/// A calendar the user subscribes to by its iCal (ICS) address. The address is a secret: it lives only here.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CalSource {
    pub id: String,
    pub name: String,
    pub url: String,
    /// #rrggbb
    pub color: String,
}

/// An event the user keeps in Aterya itself. start/end are milliseconds since the epoch.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LocalEvent {
    pub id: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub layout: LayoutCfg,
    pub theme: ThemeCfg,
    /// Clicking an app in the dock moves its window to the primary monitor.
    pub bring_to_main: bool,
    /// Modifier that must be held for that: "always" or one of KEYS.
    pub bring_key: String,
    pub floating: Vec<FloatCfg>,
    /// Bluetooth devices by name: shown on the dock or not, and the chosen icon.
    pub bt_prefs: std::collections::HashMap<String, BtPref>,
    /// Calendars subscribed to, and the user's own events.
    pub cal_sources: Vec<CalSource>,
    /// Devices of the battery widget the user decided on: built-in ids ("mouse", "headset", "keyboard") and
    /// Bluetooth devices ("bt:<name>"); true = shown. Not listed: built-ins show, Bluetooth devices wait to be asked about.
    pub bat_prefs: std::collections::HashMap<String, bool>,
    pub cal_local: Vec<LocalEvent>,
    /// Per-widget size factor (0.6..=3.0), missing means 1.
    pub scales: std::collections::HashMap<String, f64>,
    /// Brightness widget: "row" (side by side) or "column" (stacked).
    pub brightness_layout: String,
    /// Open-app marker under dock icons: "dot", "bigdot" or "line".
    pub indicator: String,
    /// Look of the start button: "windows", "classic", "outline", "rounded", "aterya", "grid" or "dots".
    pub start_icon: String,
    /// Device name of the screen the dock sits on (e.g. `\.\DISPLAY2`); empty means the primary screen.
    pub dock_monitor: String,
    /// Screen edge the dock sits on: "bottom" (also when empty) or "top".
    pub dock_edge: String,
    /// Screen numbering chosen by the user: device names, first = screen 1. Screens not listed follow, left to right.
    pub monitor_order: Vec<String>,
    /// Dock size factor (0.7..=1.5); 0 (missing) means 1.
    pub dock_scale: f64,
    /// Gap between the dock and the screen edge in CSS px (0..=24); missing means 4.
    pub dock_gap: Option<f64>,
    /// The dock slides away from the screen edge until the pointer or the Windows key calls it back.
    pub dock_autohide: bool,
    /// App icons lift and grow under the pointer; missing means on.
    pub hover_lift: Option<bool>,
    /// The clock widget shows the date next to the time; missing means yes.
    pub clock_date: Option<bool>,
    /// Thickness of the divider lines on the dock in px (1..=6); missing means 1.
    pub divider_width: Option<f64>,
    /// App icon size in px (20..=44, missing = 30) and the gap between icons (0..=16, missing = 6).
    pub icon_size: Option<f64>,
    pub icon_gap: Option<f64>,
    /// Dock look: corner radius in px (0..=32, missing 22), border width in px (0..=4, missing 1), text color and
    /// accent as #rrggbb (empty = the theme's own).
    pub dock_radius: Option<f64>,
    pub dock_border: Option<f64>,
    /// Look for new releases automatically (missing = yes) and install them without asking (default no).
    pub auto_update_check: Option<bool>,
    pub auto_update_install: bool,
    pub dock_text: String,
    pub dock_accent: String,
    /// Start with Windows: None until the first run decided, then the user's choice.
    pub autostart: Option<bool>,
    pub pins: Vec<Pin>,
    /// Ids of Windows-taskbar pins the user removed from the dock; they stay off the dock.
    pub hidden_pins: Vec<String>,
    pub weather: WeatherCfg,
    pub poll_minutes: PollCfg,
}

impl Config {
    /// Defaults when the file is missing; defaults plus a warning when it is invalid.
    pub fn load(path: &Path) -> Config {
        let Ok(text) = std::fs::read_to_string(path) else { return Config::default() };
        serde_json::from_str(&text).unwrap_or_else(|e| {
            log::warn!("invalid config {}: {e}", path.display());
            Config::default()
        })
    }

    /// Defaults only when the file is missing; Err on read or parse failure so callers never overwrite a bad file.
    pub fn try_load(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| e.to_string()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Atomic: writes `<path>.tmp` then renames over the target.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("custom_taskbar").join("config.json")
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn layout_sanitized() {
        let l = LayoutCfg { order: vec!["clock".into(), "bogus".into(), "clock".into(), "weather".into()], hidden: vec!["x".into(), "tray".into()], dividers: vec!["clock".into(), "nope".into()] }.sanitized();
        assert_eq!(l.dividers, ["clock"]);
        assert_eq!(l.order[..3], ["apps", "clock", "weather"]);
        assert_eq!(l.order.len(), WIDGETS.len());
        assert!(l.hidden.contains(&"tray".to_string()) && !l.hidden.contains(&"x".to_string()));
    }
    #[test]
    fn theme_sanitized() {
        let t = ThemeCfg { preset: "neon".into(), opacity: 9.0 }.sanitized();
        assert_eq!((t.preset.as_str(), t.opacity), ("glass", 0.7));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ctb_test_{}_{}", std::process::id(), name));
        std::fs::create_dir_all(&d).unwrap();
        d.join("config.json")
    }

    #[test]
    fn missing_file_gives_defaults() {
        let c = Config::load(&tmp("missing"));
        assert_eq!(c.weather.city, "İstanbul");
        assert_eq!((c.poll_minutes.battery, c.poll_minutes.weather, c.poll_minutes.claude), (5, 15, 5));
        assert!(c.pins.is_empty());
    }

    #[test]
    fn invalid_json_gives_defaults() {
        let p = tmp("invalid");
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(Config::load(&p), Config::default());
    }

    #[test]
    fn saved_json_is_camel_case() {
        let p = tmp("camel");
        Config::default().save(&p).unwrap();
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains("pollMinutes") && !t.contains("poll_minutes"));
    }

    #[test]
    fn roundtrip_save_load() {
        let p = tmp("roundtrip");
        let c = Config {
            pins: vec![Pin { id: "C:/a.exe".into() }, Pin { id: "aumid:X!App".into() }],
            weather: WeatherCfg { city: "Ankara".into(), lat: Some(39.9), lon: Some(32.8) },
            poll_minutes: PollCfg { battery: 1, weather: 2, claude: 3 },
            ..Default::default()
        };
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p), c);
    }

    #[test]
    fn try_load_invalid_is_err() {
        let p = tmp("try_invalid");
        std::fs::write(&p, "{ not json").unwrap();
        assert!(Config::try_load(&p).is_err());
    }

    #[test]
    fn try_load_missing_is_default() {
        assert_eq!(Config::try_load(&tmp("try_missing")), Ok(Config::default()));
    }

    #[test]
    fn save_leaves_no_tmp() {
        let p = tmp("notmp");
        Config::default().save(&p).unwrap();
        Config::default().save(&p).unwrap();
        assert!(!p.with_extension("json.tmp").exists());
    }
}
