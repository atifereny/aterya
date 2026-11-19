//! Claude Code OAuth usage: poll the usage endpoint with the local Claude Code token (read-only).
use crate::claude_models::{self, TopModel};
use crate::state::Hub;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClaudeUsage {
    #[serde(rename_all = "camelCase")]
    Ok {
        five_hour: f64,
        weekly: f64,
        /// ISO timestamps when the 5-hour and weekly windows reset.
        five_hour_resets: Option<String>,
        weekly_resets: Option<String>,
        top_model: Option<TopModel>,
    },
    Expired,
    /// Server asked us to slow down (429) and there is no earlier reading to show.
    Limited,
    /// Claude Code is being run once to renew the token.
    Refreshing,
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
    let resets = |v: &serde_json::Value, k: &str| v.get(k)?.get("resets_at")?.as_str().map(str::to_string);
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) => match (pct(&v, "five_hour"), pct(&v, "seven_day")) {
            (Some(five_hour), Some(weekly)) => ClaudeUsage::Ok {
                five_hour,
                weekly,
                five_hour_resets: resets(&v, "five_hour"),
                weekly_resets: resets(&v, "seven_day"),
                top_model: None,
            },
            _ => ClaudeUsage::Unknown,
        },
        Err(_) => ClaudeUsage::Unknown,
    }
}

/// Fresh read each poll; never writes the file, never refreshes the token.
/// Second value: server-requested wait in seconds (429 Retry-After), if any.
fn fetch(client: &reqwest::blocking::Client) -> (ClaudeUsage, Option<u64>) {
    let Some(path) = std::env::var_os("USERPROFILE").map(|p| std::path::PathBuf::from(p).join(".claude").join(".credentials.json")) else {
        return (ClaudeUsage::Unknown, None);
    };
    let Some(token) = std::fs::read_to_string(path).ok().and_then(|s| read_token(&s)).map(Token) else {
        return (ClaudeUsage::Unknown, None);
    };
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {}", token.0))
        .header("anthropic-beta", "oauth-2025-04-20")
        .send();
    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            let retry = r.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
            // body is parsed, never logged
            (parse_usage(status, &r.text().unwrap_or_default()), (status == 429).then(|| retry.unwrap_or(600)))
        }
        Err(e) => {
            let msg = e.without_url().to_string();
            log::warn!("claude usage request failed: {msg}"); // error only, no request/headers
            (ClaudeUsage::Unknown, None)
        }
    }
}

fn cache_path() -> std::path::PathBuf {
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("custom_taskbar").join("claude-usage.json")
}

/// Last good reading (two percentages and a time, nothing secret). Lets restarts show a value
/// without calling the API again: the endpoint answers 429 when polled too often.
fn load_cache() -> Option<(ClaudeUsage, u64)> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(cache_path()).ok()?).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    let top_model = v.get("topModel").and_then(|t| Some(TopModel { name: t.get("name")?.as_str()?.to_string(), tokens: t.get("tokens")?.as_u64()? }));
    let usage = ClaudeUsage::Ok {
        five_hour: v.get("fiveHour")?.as_f64()?,
        weekly: v.get("weekly")?.as_f64()?,
        five_hour_resets: s("fiveHourResets"),
        weekly_resets: s("weeklyResets"),
        top_model,
    };
    Some((usage, v.get("at")?.as_u64()?))
}

fn save_cache(u: &ClaudeUsage) {
    let ClaudeUsage::Ok { five_hour, weekly, five_hour_resets, weekly_resets, top_model } = u else { return };
    let _ = std::fs::write(
        cache_path(),
        serde_json::json!({ "fiveHour": five_hour, "weekly": weekly, "fiveHourResets": five_hour_resets, "weeklyResets": weekly_resets, "topModel": top_model, "at": now_secs() })
            .to_string(),
    );
}

fn block_path() -> std::path::PathBuf {
    cache_path().with_file_name("claude-usage-blocked-until")
}

/// Epoch second until which the server asked for quiet; persists so restarts keep honoring it.
fn blocked_until() -> u64 {
    std::fs::read_to_string(block_path()).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

static FORCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static REFRESHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LAST_AUTO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const AUTO_REFRESH_GAP_SECS: u64 = 30 * 60;

/// Runs Claude Code once on its smallest model so it renews its own token. The dock never touches
/// .credentials.json. Empty working dir: no project files or instructions get loaded.
fn run_claude() {
    use std::os::windows::process::CommandExt;
    let dir = std::env::temp_dir().join("custom_taskbar_claude");
    let _ = std::fs::create_dir_all(&dir);
    let Ok(mut child) = std::process::Command::new("cmd")
        .args(["/C", "claude", "-p", "ok", "--model", "haiku"])
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
    else {
        return;
    };
    let end = std::time::Instant::now() + Duration::from_secs(90);
    while std::time::Instant::now() < end {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = child.kill();
}

fn spawn_refresh(app: AppHandle) {
    use std::sync::atomic::Ordering::SeqCst;
    if REFRESHING.swap(true, SeqCst) {
        return;
    }
    app.state::<Hub>().emit("claude://changed", ClaudeUsage::Refreshing);
    std::thread::spawn(move || {
        run_claude();
        let _ = std::fs::remove_file(block_path()); // the old rate-limit wait no longer applies
        FORCE.store(true, SeqCst);
        REFRESHING.store(false, SeqCst);
    });
}

/// Tile button: renew the token through Claude Code, then read the usage again.
#[tauri::command]
pub fn claude_refresh(app: AppHandle) {
    spawn_refresh(app);
}

pub fn start(app: AppHandle, poll_minutes: u64) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).build() else {
            return;
        };
        let poll = poll_minutes.max(1) * 60;
        let mut last_ok = load_cache();
        loop {
            let force = FORCE.swap(false, std::sync::atomic::Ordering::SeqCst);
            // A reading younger than one poll interval is reused (dock restarts must not hammer the API).
            let wait = match &last_ok {
                Some((u, at)) if !force && now_secs().saturating_sub(*at) < poll => {
                    app.state::<Hub>().emit("claude://changed", u.clone());
                    poll - now_secs().saturating_sub(*at)
                }
                _ if !force && blocked_until() > now_secs() => {
                    let shown = last_ok.as_ref().map_or(ClaudeUsage::Limited, |(u, _)| u.clone());
                    app.state::<Hub>().emit("claude://changed", shown);
                    blocked_until() - now_secs()
                }
                _ => {
                    let (u, retry) = fetch(&client);
                    match &u {
                        ClaudeUsage::Ok { .. } => {
                            let mut u = u;
                            if let ClaudeUsage::Ok { top_model, .. } = &mut u {
                                *top_model = claude_models::top_model();
                            }
                            save_cache(&u);
                            last_ok = Some((u.clone(), now_secs()));
                            app.state::<Hub>().emit("claude://changed", u);
                            poll
                        }
                        // Rate limited: keep showing the last good value and back off as asked.
                        _ if retry.is_some() => {
                            let shown = last_ok.as_ref().map_or(ClaudeUsage::Limited, |(u, _)| u.clone());
                            app.state::<Hub>().emit("claude://changed", shown);
                            let wait = retry.unwrap().max(poll);
                            let _ = std::fs::write(block_path(), (now_secs() + wait).to_string());
                            wait
                        }
                        _ => {
                            app.state::<Hub>().emit("claude://changed", u.clone());
                            // Token expired: let Claude Code renew it, at most once per AUTO_REFRESH_GAP_SECS.
                            let last = LAST_AUTO.load(std::sync::atomic::Ordering::SeqCst);
                            if u == ClaudeUsage::Expired && !force && now_secs().saturating_sub(last) > AUTO_REFRESH_GAP_SECS {
                                LAST_AUTO.store(now_secs(), std::sync::atomic::Ordering::SeqCst);
                                spawn_refresh(app.clone());
                            }
                            poll
                        }
                    }
                }
            };
            let end = std::time::Instant::now() + Duration::from_secs(wait.max(30));
            while std::time::Instant::now() < end && !FORCE.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs(1));
            }
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
        let b = r#"{"five_hour":{"utilization":62.0,"resets_at":"2026-11-17T20:00:00Z"},"seven_day":{"utilization":18.5,"resets_at":"2026-11-24T09:00:00Z"}}"#;
        let ClaudeUsage::Ok { five_hour, weekly, five_hour_resets, weekly_resets, top_model } = parse_usage(200, b) else { panic!() };
        assert_eq!((five_hour, weekly), (62.0, 18.5));
        assert_eq!(five_hour_resets.as_deref(), Some("2026-11-17T20:00:00Z"));
        assert_eq!(weekly_resets.as_deref(), Some("2026-11-24T09:00:00Z"));
        assert_eq!(top_model, None);
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
