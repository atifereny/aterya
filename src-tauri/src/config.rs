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

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub pins: Vec<Pin>,
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

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("custom_taskbar").join("config.json")
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
        };
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p), c);
    }
}
