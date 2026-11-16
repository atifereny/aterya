//! Claude Code OAuth usage: poll the usage endpoint with the local Claude Code token (read-only).
use crate::state::Hub;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClaudeUsage {
    #[serde(rename_all = "camelCase")]
    Ok { five_hour: f64, weekly: f64 },
    Expired,
    Unknown,
}

/// Token wrapper whose Debug never reveals the secret.
pub struct Token(String);
impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

pub fn read_token(credentials_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(credentials_json).ok()?;
    Some(v.get("claudeAiOauth")?.get("accessToken")?.as_str()?.to_string())
}

pub fn parse_usage(status: u16, body: &str) -> ClaudeUsage {
    match status {
        200 => {}
        401 => return ClaudeUsage::Expired,
        _ => return ClaudeUsage::Unknown,
    }
    let pct = |v: &serde_json::Value, k: &str| v.get(k)?.get("utilization")?.as_f64();
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) => match (pct(&v, "five_hour"), pct(&v, "seven_day")) {
            (Some(five_hour), Some(weekly)) => ClaudeUsage::Ok { five_hour, weekly },
            _ => ClaudeUsage::Unknown,
        },
        Err(_) => ClaudeUsage::Unknown,
    }
}

/// Fresh read each poll; never writes the file, never refreshes the token.
fn fetch(client: &reqwest::blocking::Client) -> ClaudeUsage {
    let Some(path) = std::env::var_os("USERPROFILE").map(|p| std::path::PathBuf::from(p).join(".claude").join(".credentials.json")) else {
        return ClaudeUsage::Unknown;
    };
    let Some(token) = std::fs::read_to_string(path).ok().and_then(|s| read_token(&s)).map(Token) else {
        return ClaudeUsage::Unknown;
    };
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {}", token.0))
        .header("anthropic-beta", "oauth-2025-04-20")
        .send();
    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            // body is parsed, never logged
            parse_usage(status, &r.text().unwrap_or_default())
        }
        Err(e) => {
            log::warn!("claude usage request failed: {}", e.without_url()); // error only, no request/headers
            ClaudeUsage::Unknown
        }
    }
}

pub fn start(app: AppHandle, poll_minutes: u64) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).build() else {
            return;
        };
        loop {
            app.state::<Hub>().emit("claude://changed", fetch(&client));
            std::thread::sleep(Duration::from_secs(poll_minutes.max(1) * 60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_ok() {
        assert_eq!(read_token(r#"{"claudeAiOauth":{"accessToken":"fake-tok"}}"#), Some("fake-tok".into()));
    }

    #[test]
    fn token_missing_or_malformed() {
        assert_eq!(read_token(r#"{"other":{}}"#), None);
        assert_eq!(read_token("garbage"), None);
    }

    #[test]
    fn usage_ok() {
        let b = r#"{"five_hour":{"utilization":62.0},"seven_day":{"utilization":18.5}}"#;
        assert_eq!(parse_usage(200, b), ClaudeUsage::Ok { five_hour: 62.0, weekly: 18.5 });
        let j = serde_json::to_string(&parse_usage(200, b)).unwrap();
        assert_eq!(j, r#"{"kind":"ok","fiveHour":62.0,"weekly":18.5}"#);
    }

    #[test]
    fn usage_statuses() {
        assert_eq!(parse_usage(401, ""), ClaudeUsage::Expired);
        assert_eq!(parse_usage(429, ""), ClaudeUsage::Unknown);
        assert_eq!(parse_usage(500, ""), ClaudeUsage::Unknown);
        assert_eq!(parse_usage(200, "garbage"), ClaudeUsage::Unknown);
    }

    #[test]
    fn token_not_in_debug() {
        let t = Token("fake-secret-123".into());
        assert!(!format!("{:?}", t).contains("fake-secret-123"));
    }
}
