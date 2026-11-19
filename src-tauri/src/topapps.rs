//! Top CPU and RAM apps: processes grouped by exe, sampled every 2 s. CPU is the share of all cores,
//! RAM the private working set summed over the app's processes (like Task Manager's Memory column).
use serde::Serialize;
use std::{collections::HashMap, sync::Mutex, time::{Duration, Instant}};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE},
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
        System::{
            Com::{CoInitializeEx, COINIT_APARTMENTTHREADED},
            Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2},
            SystemInformation::{GetSystemInfo, SYSTEM_INFO},
            Threading::{GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
        },
    },
    core::{HSTRING, PWSTR},
};

#[derive(Serialize, Clone, Default)]
pub struct Row {
    name: String,
    icon: String,
    /// Percent for CPU, bytes for RAM.
    value: f64,
}
#[derive(Serialize, Clone, Default)]
pub struct Top {
    cpu: Vec<Row>,
    ram: Vec<Row>,
}

static TOP: Mutex<Option<Top>> = Mutex::new(None);

fn ticks(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn image_path(h: HANDLE) -> Option<String> {
    let mut buf = [0u16; 520];
    let mut n = buf.len() as u32;
    unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n) }.ok()?;
    Some(String::from_utf16_lossy(&buf[..n as usize]))
}

/// "Google Chrome" from the exe's version resource.
fn description(path: &str) -> Option<String> {
    let p = HSTRING::from(path);
    unsafe {
        let size = GetFileVersionInfoSizeW(&p, None);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        GetFileVersionInfoW(&p, None, size, buf.as_mut_ptr() as *mut _).ok()?;
        let (mut ptr, mut len) = (std::ptr::null_mut(), 0u32);
        if !VerQueryValueW(buf.as_ptr() as *const _, &HSTRING::from(r"\VarFileInfo\Translation"), &mut ptr, &mut len).as_bool() || len < 4 {
            return None;
        }
        let t = std::slice::from_raw_parts(ptr as *const u16, 2);
        let key = HSTRING::from(format!(r"\StringFileInfo\{:04x}{:04x}\FileDescription", t[0], t[1]));
        if !VerQueryValueW(buf.as_ptr() as *const _, &key, &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr as *const u16, len as usize));
        let s = s.split('\0').next().unwrap_or_default().trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

fn friendly(path: &str, names: &mut HashMap<String, String>) -> String {
    names
        .entry(path.to_string())
        .or_insert_with(|| {
            description(path)
                .unwrap_or_else(|| std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
        })
        .clone()
}

#[derive(Default)]
struct Agg {
    path: String,
    cpu_ticks: u64,
    ram: u64,
}

fn sample(prev: &mut HashMap<u32, u64>, last: &mut Instant, cores: f64, names: &mut HashMap<String, String>) -> Top {
    let now = Instant::now();
    let elapsed = now.duration_since(*last).as_secs_f64() * 1e7; // 100 ns units
    *last = now;
    let mut groups: HashMap<String, Agg> = HashMap::new();
    let mut seen: HashMap<u32, u64> = HashMap::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return Top::default() };
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let pid = e.th32ProcessID;
            if pid > 4 {
                if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    if let Some(path) = image_path(h) {
                        let (mut c, mut x, mut k, mut u) = Default::default();
                        let t = GetProcessTimes(h, &mut c, &mut x, &mut k, &mut u).is_ok().then(|| ticks(k) + ticks(u)).unwrap_or(0);
                        let mut m = PROCESS_MEMORY_COUNTERS_EX2 { cb: size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32, ..Default::default() };
                        let ram = GetProcessMemoryInfo(h, &mut m as *mut _ as *mut PROCESS_MEMORY_COUNTERS, m.cb).is_ok().then_some(m.PrivateWorkingSetSize as u64).unwrap_or(0);
                        // One row per app name, so an app with several exes (League client) is not listed twice.
                        let g = groups.entry(friendly(&path.to_lowercase(), names)).or_default();
                        if g.path.is_empty() {
                            g.path = path.to_lowercase();
                        }
                        g.ram += ram;
                        if let Some(&p) = prev.get(&pid) {
                            g.cpu_ticks += t.saturating_sub(p);
                        }
                        seen.insert(pid, t);
                    }
                    let _ = CloseHandle(h);
                }
            }
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    *prev = seen;
    let mut v: Vec<(&String, &Agg)> = groups.iter().collect();
    let mut rows = |key: fn(&Agg) -> f64, scale: f64| {
        v.sort_by(|a, b| key(b.1).total_cmp(&key(a.1)));
        v.iter()
            .take(3)
            .map(|(n, a)| Row { name: (*n).clone(), icon: crate::apps::icons::icon_for(&a.path, None), value: key(a) * scale })
            .collect::<Vec<_>>()
    };
    Top { cpu: rows(|a| a.cpu_ticks as f64, 100.0 / (elapsed * cores).max(1.0)), ram: rows(|a| a.ram as f64, 1.0) }
}

pub fn start() {
    std::thread::spawn(|| {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let mut si = SYSTEM_INFO::default();
        unsafe { GetSystemInfo(&mut si) };
        let cores = si.dwNumberOfProcessors.max(1) as f64;
        let (mut prev, mut last, mut names) = (HashMap::new(), Instant::now(), HashMap::new());
        loop {
            std::thread::sleep(Duration::from_secs(2));
            *TOP.lock().unwrap() = Some(sample(&mut prev, &mut last, cores, &mut names));
        }
    });
}

#[tauri::command]
pub fn top_get() -> Top {
    TOP.lock().unwrap().clone().unwrap_or_default()
}

/// Task Manager needs elevation: go through the shell so Windows shows its prompt.
#[tauri::command]
pub fn open_task_manager() {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("cmd").raw_arg("/C start \"\" taskmgr").creation_flags(0x0800_0000).spawn();
}
