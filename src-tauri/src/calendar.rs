//! Calendar: events from iCal (ICS) feeds the user subscribes to (Google Calendar's "secret address in iCal
//! format", Outlook and iCloud public links, ...), read-only, plus events the user keeps locally. The feeds are
//! fetched now and then; recurring events are expanded for the month the page asks for.
use crate::config::{CalSource, LocalEvent};
use crate::popup::Popup;
use crate::settings::{current, save_and_emit};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration as Wait;
use tauri::{AppHandle, Emitter};

static POPUP: Popup = Popup::new("cal", 420, "cal://open");

/// One event of a feed as written in it; recurring ones are expanded on demand.
#[derive(Clone, Debug, Default)]
struct Raw {
    uid: String,
    title: String,
    location: String,
    start: i64,
    end: i64,
    all_day: bool,
    /// DTSTART, RRULE and EXDATE lines for the recurrence engine; None for a single event.
    rule: Option<String>,
    /// For an edited occurrence of a recurring event: the start it replaces.
    replaces: Option<i64>,
}

#[derive(Default)]
struct Feed {
    events: Vec<Raw>,
    error: Option<String>,
}

static FEEDS: Mutex<Option<HashMap<String, Feed>>> = Mutex::new(None);

fn feeds<R>(f: impl FnOnce(&mut HashMap<String, Feed>) -> R) -> R {
    let mut g = FEEDS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

// ---- ICS parsing ----

/// "NAME;PARAM=x:value" -> (NAME, params, value).
fn split_line(line: &str) -> Option<(String, String, String)> {
    let mut in_quote = false;
    let colon = line.char_indices().find(|&(_, c)| {
        if c == '"' {
            in_quote = !in_quote;
        }
        c == ':' && !in_quote
    })?;
    let (head, value) = (&line[..colon.0], &line[colon.0 + 1..]);
    let (name, params) = head.split_once(';').unwrap_or((head, ""));
    Some((name.to_uppercase(), params.to_string(), value.to_string()))
}

fn param<'a>(params: &'a str, key: &str) -> Option<&'a str> {
    params.split(';').find_map(|p| p.strip_prefix(&format!("{key}=")))
}

/// An iCal date or date-time as (milliseconds since the epoch, is it a whole day).
fn when(params: &str, value: &str) -> Option<(i64, bool)> {
    let v = value.trim();
    if param(params, "VALUE") == Some("DATE") || (v.len() == 8 && !v.contains('T')) {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        let t = Local.from_local_datetime(&d.and_time(NaiveTime::MIN)).earliest()?;
        return Some((t.timestamp_millis(), true));
    }
    let naive = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    let ms = if v.ends_with('Z') {
        Utc.from_utc_datetime(&naive).timestamp_millis()
    } else if let Some(tz) = param(params, "TZID").and_then(|t| t.trim_matches('"').parse::<chrono_tz::Tz>().ok()) {
        tz.from_local_datetime(&naive).earliest()?.timestamp_millis()
    } else {
        Local.from_local_datetime(&naive).earliest()?.timestamp_millis() // floating, or a Windows zone name we do not know
    };
    Some((ms, false))
}

fn duration_ms(v: &str) -> Option<i64> {
    // PnDTnHnMnS (and PnW)
    let s = v.trim().strip_prefix('P')?;
    let (mut total, mut num) = (0i64, String::new());
    for c in s.chars() {
        match c {
            '0'..='9' => num.push(c),
            'T' => {}
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += n * match c {
                    'W' => 7 * 86_400_000,
                    'D' => 86_400_000,
                    'H' => 3_600_000,
                    'M' => 60_000,
                    'S' => 1000,
                    _ => return None,
                };
            }
        }
    }
    Some(total)
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\N", "\n").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// The event being read: the fields of `Raw` plus what is only needed to finish it.
#[derive(Default)]
struct Building {
    ev: Raw,
    rule_lines: Vec<String>,
    dtstart: Option<(String, String)>,
    dtend: Option<i64>,
    duration: Option<i64>,
    cancelled: bool,
}

fn finish(b: Building) -> Option<Raw> {
    let Building { mut ev, rule_lines, dtstart, dtend, duration, cancelled } = b;
    let (params, value) = dtstart?;
    let (start, all_day) = when(&params, &value)?;
    if cancelled {
        return None;
    }
    ev.start = start;
    ev.all_day = all_day;
    ev.end = dtend.unwrap_or_else(|| match duration {
        Some(d) => start + d,
        None if all_day => start + 86_400_000,
        None => start,
    });
    if rule_lines.iter().any(|l| l.starts_with("RRULE")) {
        // All-day starts are written as local midnight so the engine needs no date-only support.
        let dt = if all_day {
            format!("DTSTART:{}T000000", value.trim())
        } else if params.is_empty() {
            format!("DTSTART:{}", value.trim())
        } else {
            format!("DTSTART;{params}:{}", value.trim())
        };
        ev.rule = Some(std::iter::once(dt).chain(rule_lines).collect::<Vec<_>>().join("\n"));
    }
    Some(ev)
}

fn parse_ics(text: &str) -> Vec<Raw> {
    let text = text.replace("\r\n", "\n").replace("\n ", "").replace("\n\t", "");
    let mut out = Vec::new();
    let mut cur: Option<Building> = None;
    for line in text.lines() {
        if line == "BEGIN:VEVENT" {
            cur = Some(Building::default());
            continue;
        }
        if line == "END:VEVENT" {
            out.extend(cur.take().and_then(finish));
            continue;
        }
        let Some(b) = cur.as_mut() else { continue };
        let Some((name, params, value)) = split_line(line) else { continue };
        match name.as_str() {
            "UID" => b.ev.uid = value,
            "SUMMARY" => b.ev.title = unescape(&value),
            "LOCATION" => b.ev.location = unescape(&value),
            "DTSTART" => b.dtstart = Some((params, value)),
            "DTEND" => b.dtend = when(&params, &value).map(|w| w.0),
            "DURATION" => b.duration = duration_ms(&value),
            "STATUS" => b.cancelled = value.eq_ignore_ascii_case("CANCELLED"),
            "RECURRENCE-ID" => b.ev.replaces = when(&params, &value).map(|w| w.0),
            "RRULE" => b.rule_lines.push(format!("RRULE:{value}")),
            "EXDATE" => {
                // Written as absolute times so zones and all-day dates need no care in the engine.
                for v in value.split(',') {
                    if let Some(t) = when(&params, v).and_then(|(ms, _)| DateTime::<Utc>::from_timestamp_millis(ms)) {
                        b.rule_lines.push(format!("EXDATE:{}", t.format("%Y%m%dT%H%M%SZ")));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

// ---- expanding for a window ----

/// Events of `raw` that touch [from, to) (milliseconds).
fn expand(raw: &[Raw], from: i64, to: i64) -> Vec<(Raw, i64, i64)> {
    use rrule::{RRuleSet, Tz};
    let replaced: Vec<(&str, i64)> = raw.iter().filter_map(|r| r.replaces.map(|t| (r.uid.as_str(), t))).collect();
    let mut out = Vec::new();
    for r in raw {
        let len = (r.end - r.start).max(0);
        match &r.rule {
            None => {
                if r.start < to && r.end.max(r.start + 1) > from {
                    out.push((r.clone(), r.start, r.end));
                }
            }
            Some(rule) => {
                let Ok(set) = rule.parse::<RRuleSet>() else {
                    if r.start < to && r.end > from {
                        out.push((r.clone(), r.start, r.end));
                    }
                    continue;
                };
                let (Some(a), Some(b)) = (Tz::UTC.timestamp_millis_opt(from - len).single(), Tz::UTC.timestamp_millis_opt(to).single()) else { continue };
                for d in set.after(a).before(b).all(400).dates {
                    let s = d.timestamp_millis();
                    if replaced.iter().any(|&(u, t)| u == r.uid && t == s) {
                        continue;
                    }
                    out.push((r.clone(), s, s + len));
                }
            }
        }
    }
    out
}

// ---- fetching ----

fn fetch(url: &str) -> Result<String, String> {
    let url = url.trim().replacen("webcal://", "https://", 1);
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Adres https:// ile başlamalı".into());
    }
    let client = reqwest::blocking::Client::builder().timeout(Wait::from_secs(20)).user_agent("Aterya").build().map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().map_err(|_| "Takvim adresine ulaşılamadı".to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Takvim adresi hata verdi ({})", resp.status().as_u16()));
    }
    if resp.content_length().is_some_and(|n| n > 30_000_000) {
        return Err("Takvim dosyası çok büyük".into());
    }
    let text = resp.text().map_err(|_| "Takvim okunamadı".to_string())?;
    if !text.contains("BEGIN:VCALENDAR") {
        return Err("Bu adres bir takvim (ICS) bağlantısı değil".into());
    }
    Ok(text)
}

fn refresh_one(s: &CalSource) {
    let res = fetch(&s.url);
    feeds(|f| {
        let e = f.entry(s.id.clone()).or_default();
        match res {
            Ok(t) => {
                e.events = parse_ics(&t);
                e.error = None;
            }
            // Keep the events of the last good fetch; only report the problem.
            Err(msg) => e.error = Some(msg),
        }
    });
}

fn refresh_all(app: &AppHandle) {
    let Ok(c) = current() else { return };
    for s in &c.cal_sources {
        refresh_one(s);
        let _ = app.emit("cal://changed", ());
    }
}

/// Fetch the feeds at start and every 15 minutes.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        refresh_all(&app);
        std::thread::sleep(Wait::from_secs(15 * 60));
    });
}

pub fn warm(app: &AppHandle) {
    POPUP.warm(app);
}

// ---- commands ----

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CalEvent {
    id: String,
    title: String,
    location: String,
    start: i64,
    end: i64,
    all_day: bool,
    color: String,
    source: String,
    local: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    id: String,
    name: String,
    color: String,
    error: Option<String>,
    count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalState {
    sources: Vec<SourceInfo>,
    events: Vec<CalEvent>,
}

fn state_blocking(from: i64, to: i64) -> CalState {
    let cfg = current().unwrap_or_default();
    let mut events = Vec::new();
    let mut sources = Vec::new();
    for s in &cfg.cal_sources {
        let (raw, error) = feeds(|f| f.get(&s.id).map(|x| (x.events.clone(), x.error.clone())).unwrap_or_default());
        sources.push(SourceInfo { id: s.id.clone(), name: s.name.clone(), color: s.color.clone(), error, count: raw.len() });
        for (r, start, end) in expand(&raw, from, to) {
            events.push(CalEvent { id: format!("{}:{}:{}", s.id, r.uid, start), title: r.title, location: r.location, start, end, all_day: r.all_day, color: s.color.clone(), source: s.name.clone(), local: false });
        }
    }
    for l in &cfg.cal_local {
        if l.start < to && l.end.max(l.start + 1) > from {
            events.push(CalEvent { id: l.id.clone(), title: l.title.clone(), location: String::new(), start: l.start, end: l.end, all_day: l.all_day, color: l.color.clone(), source: "Aterya".into(), local: true });
        }
    }
    events.sort_by_key(|e| (e.start, e.title.clone()));
    CalState { sources, events }
}

/// The calendars and every event between `from` and `to` (milliseconds since the epoch).
#[tauri::command]
pub async fn cal_state(from: i64, to: i64) -> CalState {
    tauri::async_runtime::spawn_blocking(move || state_blocking(from, to)).await.unwrap_or(CalState { sources: vec![], events: vec![] })
}

/// Fetch every feed again now.
#[tauri::command]
pub async fn cal_refresh(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || refresh_all(&app)).await;
}

fn color_ok(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|d| d.is_ascii_hexdigit())
}

#[tauri::command]
pub async fn cal_source_add(app: AppHandle, name: String, url: String, color: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Takvime bir ad ver".into());
    }
    let color = if color_ok(&color) { color } else { "#4d8dff".into() };
    let mut c = current()?;
    let url = url.trim().replacen("webcal://", "https://", 1);
    if c.cal_sources.iter().any(|s| s.url == url) {
        return Err("Bu takvim zaten ekli".into());
    }
    let src = CalSource { id: Local::now().timestamp_millis().to_string(), name, url, color };
    let s2 = src.clone();
    // Fetch first: a wrong address is reported now instead of showing an empty calendar.
    tauri::async_runtime::spawn_blocking(move || fetch(&s2.url).map(|t| (s2.id.clone(), parse_ics(&t)))).await.map_err(|e| e.to_string())?.map(|(id, ev)| {
        feeds(|f| {
            f.insert(id, Feed { events: ev, error: None });
        })
    })?;
    c.cal_sources.push(src);
    save_and_emit(&app, &c)?;
    let _ = app.emit("cal://changed", ());
    Ok(())
}

#[tauri::command]
pub async fn cal_source_remove(app: AppHandle, id: String) -> Result<(), String> {
    let mut c = current()?;
    c.cal_sources.retain(|s| s.id != id);
    feeds(|f| {
        f.remove(&id);
    });
    save_and_emit(&app, &c)?;
    let _ = app.emit("cal://changed", ());
    Ok(())
}

#[tauri::command]
pub async fn cal_event_add(app: AppHandle, title: String, start: i64, end: i64, all_day: bool, color: String) -> Result<(), String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Etkinliğe bir ad ver".into());
    }
    let mut c = current()?;
    if c.cal_local.len() >= 2000 {
        return Err("Çok fazla yerel etkinlik var".into());
    }
    let end = if end < start { start } else { end };
    let color = if color_ok(&color) { color } else { "#ff8f3d".into() };
    c.cal_local.push(LocalEvent { id: format!("local-{}", Local::now().timestamp_millis()), title, start, end, all_day, color });
    save_and_emit(&app, &c)?;
    let _ = app.emit("cal://changed", ());
    Ok(())
}

/// Rename a calendar or change its color.
#[tauri::command]
pub async fn cal_source_update(app: AppHandle, id: String, name: String, color: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Takvime bir ad ver".into());
    }
    let mut c = current()?;
    let s = c.cal_sources.iter_mut().find(|s| s.id == id).ok_or("Takvim bulunamadı")?;
    s.name = name;
    if color_ok(&color) {
        s.color = color;
    }
    save_and_emit(&app, &c)?;
    let _ = app.emit("cal://changed", ());
    Ok(())
}

/// Change one of the user's own events.
#[tauri::command]
pub async fn cal_event_update(app: AppHandle, id: String, title: String, start: i64, end: i64, all_day: bool, color: String) -> Result<(), String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Etkinliğe bir ad ver".into());
    }
    let mut c = current()?;
    let e = c.cal_local.iter_mut().find(|e| e.id == id).ok_or("Etkinlik bulunamadı")?;
    (e.title, e.start, e.end, e.all_day) = (title, start, end.max(start), all_day);
    if color_ok(&color) {
        e.color = color;
    }
    save_and_emit(&app, &c)?;
    let _ = app.emit("cal://changed", ());
    Ok(())
}

#[tauri::command]
pub async fn cal_event_delete(app: AppHandle, id: String) -> Result<(), String> {
    let mut c = current()?;
    c.cal_local.retain(|e| e.id != id);
    save_and_emit(&app, &c)?;
    let _ = app.emit("cal://changed", ());
    Ok(())
}

#[tauri::command]
pub async fn cal_toggle(app: AppHandle, icon_cx: f64) -> Result<(), String> {
    POPUP.toggle(app, icon_cx, 600.0).await
}

#[tauri::command]
pub async fn cal_fit(app: AppHandle, height: f64) -> Result<(), String> {
    POPUP.fit(app, height).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n\
BEGIN:VEVENT\r\nUID:a1\r\nSUMMARY:Toplant\u{0131}\r\nDTSTART:20261124T090000Z\r\nDTEND:20261124T100000Z\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:a2\r\nSUMMARY:Tatil\r\nDTSTART;VALUE=DATE:20261125\r\nDTEND;VALUE=DATE:20261127\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:a3\r\nSUMMARY:Haftal\u{0131}k\r\nDTSTART;TZID=Europe/Istanbul:20261102T180000\r\nDTEND;TZID=Europe/Istanbul:20261102T190000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO\r\nEXDATE;TZID=Europe/Istanbul:20261116T180000\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:a4\r\nSUMMARY:İptal\r\nDTSTART:20261124T090000Z\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    fn ms(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }

    #[test]
    fn parses_single_and_all_day() {
        let ev = parse_ics(ICS);
        assert_eq!(ev.len(), 3, "cancelled event dropped");
        let a = ev.iter().find(|e| e.uid == "a1").unwrap();
        assert_eq!((a.start, a.end, a.all_day), (ms("2026-11-24T09:00:00Z"), ms("2026-11-24T10:00:00Z"), false));
        let t = ev.iter().find(|e| e.uid == "a2").unwrap();
        assert!(t.all_day);
        assert_eq!(t.end - t.start, 2 * 86_400_000);
    }

    #[test]
    fn expands_weekly_with_exdate() {
        let ev = parse_ics(ICS);
        let got = expand(&ev, ms("2026-11-01T00:00:00Z"), ms("2026-12-01T00:00:00Z"));
        let weekly: Vec<i64> = got.iter().filter(|(r, _, _)| r.uid == "a3").map(|&(_, s, _)| s).collect();
        // Mondays 2, 9, 23, 30 November at 18:00 Istanbul (UTC+3); the 16th is excluded.
        let want: Vec<i64> = ["2026-11-02", "2026-11-09", "2026-11-23", "2026-11-30"].iter().map(|d| ms(&format!("{d}T15:00:00Z"))).collect();
        assert_eq!(weekly, want);
    }

    #[test]
    fn window_filters_single_events() {
        let ev = parse_ics(ICS);
        assert!(expand(&ev, ms("2026-12-01T00:00:00Z"), ms("2026-12-02T00:00:00Z")).iter().all(|(r, _, _)| r.uid == "a3"));
    }

    #[test]
    fn durations() {
        assert_eq!(duration_ms("PT1H30M"), Some(5_400_000));
        assert_eq!(duration_ms("P1D"), Some(86_400_000));
    }
}
