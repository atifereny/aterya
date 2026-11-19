//! Brightness per display: external monitors over DDC/CI, the laptop panel over WMI.
//! Displays are numbered left to right (then top to bottom), the way they sit on the desk.
use serde::Serialize;
use std::sync::Mutex;
use windows::Win32::{
    Devices::Display::{
        DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR,
        SetMonitorBrightness, PHYSICAL_MONITOR,
    },
    Foundation::{LPARAM, RECT},
    Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
};
use windows_core::BOOL;

/// Calls to the monitors are slow and must not overlap.
static BUSY: Mutex<()> = Mutex::new(());

/// Index of the laptop panel (WMI); DDC monitors are 0, 1, 2... in screen order.
const INTERNAL: usize = 1000;

#[derive(Serialize)]
pub struct Display {
    /// Position in the left-to-right order; also what `brightness_set` takes.
    index: usize,
    percent: u32,
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    unsafe { (*(lp.0 as *mut Vec<HMONITOR>)).push(h) };
    BOOL(1)
}

/// Opens every DDC-capable physical monitor (ordered by screen position), runs `f`, closes them.
fn with_all<R>(f: impl FnOnce(&[PHYSICAL_MONITOR]) -> R) -> R {
    let mut hs: Vec<HMONITOR> = vec![];
    let mut all: Vec<PHYSICAL_MONITOR> = vec![];
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut hs as *mut _ as isize));
        let pos = |h: &HMONITOR| {
            let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(*h, &mut mi);
            (mi.rcMonitor.left, mi.rcMonitor.top)
        };
        hs.sort_by_key(pos);
        for h in hs {
            let mut n = 0u32;
            if GetNumberOfPhysicalMonitorsFromHMONITOR(h, &mut n).is_err() || n == 0 {
                continue;
            }
            let mut ms = vec![PHYSICAL_MONITOR::default(); n as usize];
            if GetPhysicalMonitorsFromHMONITOR(h, &mut ms).is_ok() {
                all.extend(ms);
            }
        }
        let r = f(&all);
        let _ = DestroyPhysicalMonitors(&all);
        r
    }
}

/// (min, current, max) of one monitor, None when it does not answer DDC/CI.
fn read(m: &PHYSICAL_MONITOR) -> Option<(u32, u32, u32)> {
    let (mut lo, mut cur, mut hi) = (0u32, 0u32, 0u32);
    let ok = unsafe { GetMonitorBrightness(m.hPhysicalMonitor, &mut lo, &mut cur, &mut hi) } != 0;
    (ok && hi > lo).then_some((lo, cur, hi))
}

fn powershell(script: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The laptop panel's brightness, None on desktops.
fn internal_get() -> Option<u32> {
    powershell("(Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorBrightness -ErrorAction Stop).CurrentBrightness")?.parse().ok()
}

fn get_blocking() -> Vec<Display> {
    let _g = BUSY.lock().unwrap();
    let mut v: Vec<Display> = with_all(|ms| {
        ms.iter()
            .enumerate()
            .filter_map(|(index, m)| read(m).map(|(lo, cur, hi)| Display { index, percent: cur.saturating_sub(lo) * 100 / (hi - lo) }))
            .collect()
    });
    if let Some(percent) = internal_get() {
        v.push(Display { index: INTERNAL, percent });
    }
    v
}

fn set_blocking(index: usize, pct: u32) {
    let _g = BUSY.lock().unwrap();
    let pct = pct.min(100);
    if index == INTERNAL {
        let _ = powershell(&format!(
            "Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorBrightnessMethods -ErrorAction Stop | Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{Timeout=1;Brightness={pct}}}"
        ));
        return;
    }
    with_all(|ms| {
        if let Some(m) = ms.get(index) {
            if let Some((lo, _, hi)) = read(m) {
                unsafe { SetMonitorBrightness(m.hPhysicalMonitor, lo + pct * (hi - lo) / 100) };
            }
        }
    });
}

/// Every display that answers, in screen order. The laptop panel (if any) comes last.
#[tauri::command]
pub async fn brightness_get() -> Vec<Display> {
    tauri::async_runtime::spawn_blocking(get_blocking).await.unwrap_or_default()
}

#[tauri::command]
pub async fn brightness_set(index: usize, percent: u32) {
    let _ = tauri::async_runtime::spawn_blocking(move || set_blocking(index, percent)).await;
}
