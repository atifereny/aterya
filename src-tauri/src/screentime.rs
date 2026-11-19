//! Screen time: seconds of active use today (input in the last minute), total and per foreground app.
//! Sampled every 5 s, saved every minute to `screentime.json` next to config.json, reset each day.
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex, time::Duration};
use windows::Win32::{
    Foundation::CloseHandle,
    System::{
        SystemInformation::GetTickCount,
        SystemInformation::GetLocalTime,
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
    },
    UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
};

const STEP: u64 = 5;
const IDLE_SECS: u32 = 60;

#[derive(Default, Clone, Serialize, Deserialize)]
struct Day {
    date: u32,
    total: u64,
    apps: HashMap<String, u64>,
}

static DAY: Mutex<Option<Day>> = Mutex::new(None);

#[derive(Serialize)]
pub struct App {
    name: String,
    secs: u64,
}
#[derive(Serialize)]
pub struct Report {
    total: u64,
    apps: Vec<App>,
}

fn path() -> std::path::PathBuf {
    crate::config::config_path().with_file_name("screentime.json")
}

fn today() -> u32 {
    let t = unsafe { GetLocalTime() };
    t.wYear as u32 * 10000 + t.wMonth as u32 * 100 + t.wDay as u32
}

fn idle_secs() -> u32 {
    let mut li = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if unsafe { GetLastInputInfo(&mut li) }.as_bool() {
        unsafe { GetTickCount() }.wrapping_sub(li.dwTime) / 1000
    } else {
        u32::MAX
    }
}

/// Foreground app name (exe stem). None for elevated or gone processes.
fn foreground() -> Option<String> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        ok.then(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned())).flatten()
    }
}

pub fn start() {
    let mut day = std::fs::read(path()).ok().and_then(|b| serde_json::from_slice::<Day>(&b).ok()).unwrap_or_default();
    if day.date != today() {
        day = Day { date: today(), ..Default::default() };
    }
    *DAY.lock().unwrap() = Some(day);
    std::thread::spawn(|| {
        let mut ticks = 0u32;
        loop {
            std::thread::sleep(Duration::from_secs(STEP));
            let app = (idle_secs() < IDLE_SECS).then(foreground);
            let mut g = DAY.lock().unwrap();
            let d = g.as_mut().unwrap();
            if d.date != today() {
                *d = Day { date: today(), ..Default::default() };
            }
            if let Some(app) = app {
                d.total += STEP;
                if let Some(a) = app {
                    *d.apps.entry(a).or_default() += STEP;
                }
            }
            ticks += 1;
            if ticks % 12 == 0 {
                let _ = std::fs::write(path(), serde_json::to_vec(d).unwrap_or_default());
            }
        }
    });
}

#[tauri::command]
pub fn screentime_get() -> Report {
    let d = DAY.lock().unwrap().clone().unwrap_or_default();
    let mut apps: Vec<App> = d.apps.into_iter().map(|(name, secs)| App { name, secs }).collect();
    apps.sort_by(|a, b| b.secs.cmp(&a.secs));
    apps.truncate(8);
    Report { total: d.total, apps }
}
