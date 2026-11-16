//! Open-Meteo weather: geocode the city once, then poll current conditions.
use crate::config::{self, WeatherCfg};
use crate::state::Hub;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub temp_c: f64,
    pub code: u16,
    pub city: String,
}

pub fn parse_geocode(body: &str) -> Option<(f64, f64)> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let r = v.get("results")?.get(0)?;
    Some((r.get("latitude")?.as_f64()?, r.get("longitude")?.as_f64()?))
}

pub fn parse_current(body: &str) -> Option<(f64, u16)> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let c = v.get("current")?;
    Some((c.get("temperature_2m")?.as_f64()?, u16::try_from(c.get("weather_code")?.as_u64()?).ok()?))
}

/// A failed fetch keeps the last good value.
fn next_weather(prev: Option<Weather>, fetched: Result<Weather, String>) -> Option<Weather> {
    match fetched {
        Ok(w) => Some(w),
        Err(e) => {
            log::warn!("weather fetch failed: {e}");
            prev
        }
    }
}

fn get(client: &reqwest::blocking::Client, url: &str, q: &[(&str, String)]) -> Result<String, String> {
    client
        .get(url)
        .query(q)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| e.to_string())
}

fn geocode(client: &reqwest::blocking::Client, city: &str) -> Result<(f64, f64), String> {
    let q = [("name", city.to_string()), ("count", "1".into()), ("language", "tr".into())];
    let body = get(client, "https://geocoding-api.open-meteo.com/v1/search", &q)?;
    parse_geocode(&body).ok_or_else(|| format!("no geocoding result for {city}"))
}

fn fetch(client: &reqwest::blocking::Client, city: &str, lat: f64, lon: f64) -> Result<Weather, String> {
    let q = [
        ("latitude", lat.to_string()),
        ("longitude", lon.to_string()),
        ("current", "temperature_2m,weather_code".into()),
    ];
    let body = get(client, "https://api.open-meteo.com/v1/forecast", &q)?;
    let (temp_c, code) = parse_current(&body).ok_or("unexpected forecast response")?;
    Ok(Weather { temp_c, code, city: city.to_string() })
}

pub fn start(app: AppHandle, cfg: WeatherCfg, poll_minutes: u64) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).build() else {
            return;
        };
        let mut coords = cfg.lat.zip(cfg.lon);
        let mut last: Option<Weather> = None;
        loop {
            if coords.is_none() {
                match geocode(&client, &cfg.city) {
                    Ok(c) => {
                        coords = Some(c);
                        // Reload so pins changed meanwhile are not overwritten.
                        let path = config::config_path();
                        let mut full = config::Config::load(&path);
                        full.weather.lat = Some(c.0);
                        full.weather.lon = Some(c.1);
                        if let Err(e) = full.save(&path) {
                            log::warn!("saving geocode failed: {e}");
                        }
                    }
                    Err(e) => log::warn!("{e}"), // emit null below, retry next poll
                }
            }
            let fetched = match coords {
                Some((lat, lon)) => fetch(&client, &cfg.city, lat, lon),
                None => Err("no coordinates".into()),
            };
            last = next_weather(last, fetched);
            app.state::<Hub>().emit("weather://changed", last.clone());
            std::thread::sleep(Duration::from_secs(poll_minutes.max(1) * 60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(t: f64) -> Weather {
        Weather { temp_c: t, code: 2, city: "X".into() }
    }

    #[test]
    fn geocode_ok() {
        assert_eq!(parse_geocode(r#"{"results":[{"latitude":41.01,"longitude":28.97}]}"#), Some((41.01, 28.97)));
    }

    #[test]
    fn geocode_empty() {
        assert_eq!(parse_geocode(r#"{}"#), None);
        assert_eq!(parse_geocode(r#"{"results":[]}"#), None);
    }

    #[test]
    fn current_ok() {
        assert_eq!(parse_current(r#"{"current":{"temperature_2m":14.2,"weather_code":2}}"#), Some((14.2, 2)));
        assert_eq!(parse_current("garbage"), None);
    }

    #[test]
    fn fetch_failure_keeps_last_value() {
        assert_eq!(next_weather(Some(w(1.0)), Err("net".into())), Some(w(1.0)));
        assert_eq!(next_weather(Some(w(1.0)), Ok(w(2.0))), Some(w(2.0)));
        assert_eq!(next_weather(None, Err("net".into())), None);
    }

    /// Real network: confirms the Open-Meteo response shapes. Run with --ignored.
    #[test]
    #[ignore]
    fn live_open_meteo() {
        let c = reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).build().unwrap();
        let (lat, lon) = geocode(&c, "İstanbul").unwrap();
        let w = fetch(&c, "İstanbul", lat, lon).unwrap();
        println!("{lat} {lon} {w:?}");
    }
}
