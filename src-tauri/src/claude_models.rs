//! Most-used model of the last 7 days, summed from Claude Code's local transcripts
//! (~/.claude/projects/**/*.jsonl). Read-only; nothing from the conversations is kept, only token counts.
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopModel {
    pub name: String,
    pub tokens: u64,
}

/// "claude-opus-5-5" -> "Opus 5.5"; dated ids ("claude-haiku-4-5-20251001") drop the date.
pub fn display_name(id: &str) -> String {
    let mut parts: Vec<&str> = id.strip_prefix("claude-").unwrap_or(id).split('-').collect();
    parts.retain(|p| !(p.len() == 8 && p.chars().all(|c| c.is_ascii_digit())));
    let Some((family, ver)) = parts.split_first() else { return id.to_string() };
    let mut f = family.chars();
    let family = f.next().map(|c| c.to_uppercase().collect::<String>() + f.as_str()).unwrap_or_default();
    if ver.is_empty() { family } else { format!("{family} {}", ver.join(".")) }
}

/// Epoch seconds -> "YYYY-MM-DDTHH:MM:SS" (UTC), comparable as a string with transcript timestamps.
pub fn iso(secs: u64) -> String {
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Tokens per model in one transcript since `since_iso`. Counts input + output + cache writes
/// (cache reads are the same context re-read every turn and would dwarf everything). A message id
/// repeats while a reply streams; the last copy wins.
fn file_totals(path: &Path, since_iso: &str) -> HashMap<String, u64> {
    let Ok(text) = std::fs::read_to_string(path) else { return HashMap::new() };
    let mut by_id: HashMap<String, (String, u64)> = HashMap::new();
    for line in text.lines().filter(|l| l.contains("\"type\":\"assistant\"")) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v.get("timestamp").and_then(|t| t.as_str()).is_none_or(|t| t < since_iso) {
            continue;
        }
        let Some(m) = v.get("message") else { continue };
        let (Some(model), Some(u)) = (m.get("model").and_then(|x| x.as_str()), m.get("usage")) else { continue };
        if model.starts_with('<') {
            continue; // "<synthetic>"
        }
        let n = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        let id = m.get("id").and_then(|x| x.as_str()).map_or_else(|| line.len().to_string(), str::to_string);
        by_id.insert(id, (model.to_string(), n("input_tokens") + n("output_tokens") + n("cache_creation_input_tokens")));
    }
    let mut out: HashMap<String, u64> = HashMap::new();
    for (model, t) in by_id.into_values() {
        *out.entry(model).or_default() += t;
    }
    out
}

fn transcripts(dir: &Path, newer_than: SystemTime, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        match e.metadata() {
            Ok(md) if md.is_dir() => transcripts(&p, newer_than, out),
            Ok(md) if p.extension().is_some_and(|x| x == "jsonl") && md.modified().is_ok_and(|m| m >= newer_than) => out.push(p),
            _ => {}
        }
    }
}

/// The model with the most tokens in the last 7 days, or None without transcripts.
pub fn top_model() -> Option<TopModel> {
    let home = std::env::var_os("USERPROFILE")?;
    let now = SystemTime::now();
    let since = now - Duration::from_secs(7 * 86400);
    let since_iso = iso(since.duration_since(UNIX_EPOCH).ok()?.as_secs());
    let mut files = Vec::new();
    transcripts(&Path::new(&home).join(".claude").join("projects"), since, &mut files);
    let mut total: HashMap<String, u64> = HashMap::new();
    for f in files {
        for (m, t) in file_totals(&f, &since_iso) {
            *total.entry(m).or_default() += t;
        }
    }
    // Models of one family+version in different dated builds are shown together.
    let mut merged: HashMap<String, u64> = HashMap::new();
    for (m, t) in total {
        *merged.entry(display_name(&m)).or_default() += t;
    }
    merged.into_iter().filter(|(_, t)| *t > 0).max_by_key(|(_, t)| *t).map(|(name, tokens)| TopModel { name, tokens })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(display_name("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(display_name("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(display_name("claude-fable-5-2"), "Fable 5.2");
        assert_eq!(display_name("claude-sonnet-5-5"), "Sonnet 5.5");
    }

    #[test]
    fn iso_format() {
        assert_eq!(iso(0), "1970-01-01T00:00:00");
        assert_eq!(iso(1_794_930_281), "2026-11-17T15:44:41");
    }

    #[test]
    fn totals_dedupe_and_filter() {
        let dir = std::env::temp_dir().join("ctb_models_test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("t.jsonl");
        let line = |id: &str, ts: &str, out: u64| {
            format!(r#"{{"type":"assistant","timestamp":"{ts}","message":{{"id":"{id}","model":"claude-opus-5-5","usage":{{"input_tokens":1,"output_tokens":{out},"cache_creation_input_tokens":10,"cache_read_input_tokens":9999}}}}}}"#)
        };
        let body = [line("a", "2026-11-17T10:00:00Z", 5), line("a", "2026-11-17T10:00:01Z", 7), line("b", "2026-01-01T00:00:00Z", 100)].join("\n");
        std::fs::write(&f, body).unwrap();
        let t = file_totals(&f, "2026-11-10T00:00:00");
        assert_eq!(t.get("claude-opus-5-5"), Some(&(1 + 7 + 10))); // last copy of "a"; "b" is too old
    }

    #[test]
    #[ignore]
    fn live_top_model() {
        let t = std::time::Instant::now();
        println!("{:?} in {:?}", top_model(), t.elapsed());
    }
}
