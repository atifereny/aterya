//! Auto-hide: the dock slides below the bottom screen edge when the pointer leaves it and comes back when the
//! pointer touches the edge or the Windows key / Start menu is used. While hidden the work area is not reserved.
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows::core::PWSTR;
use windows::Win32::{
    Foundation::{CloseHandle, HWND, POINT, RECT},
    System::Threading::{GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LWIN, VK_RWIN},
        WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE},
    },
};

const DWELL: Duration = Duration::from_millis(450);
const BAND_SLACK: i32 = 8;

fn foreground_exe() -> (u32, String) {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        let name = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok().and_then(|p| {
            let mut buf = [0u16; 520];
            let mut n = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
            let _ = CloseHandle(p);
            ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
        });
        (pid, name.unwrap_or_default().rsplit('\\').next().unwrap_or("").to_lowercase())
    }
}

/// Reasons to keep (or bring) the dock up besides the pointer: the Windows key, the Start menu or search,
/// one of our own windows (settings, a popup) in front, or a window preview showing.
fn wanted_for_other_reasons() -> bool {
    let win = unsafe { GetAsyncKeyState(VK_LWIN.0 as i32) < 0 || GetAsyncKeyState(VK_RWIN.0 as i32) < 0 };
    if win || crate::apps::preview::shown() {
        return true;
    }
    let (pid, exe) = foreground_exe();
    pid == unsafe { GetCurrentProcessId() } || matches!(exe.as_str(), "startmenuexperiencehost.exe" | "searchhost.exe" | "shellexperiencehost.exe")
}

/// Slide the dock along its screen-normal axis (x for a side dock, y otherwise) from `from` to `to`.
fn slide(h: HWND, from: i32, to: i32, vertical: bool) {
    let mut rc = RECT::default();
    if unsafe { GetWindowRect(h, &mut rc) }.is_err() {
        return;
    }
    const STEPS: i32 = 10;
    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        let e = 1.0 - (1.0 - t).powi(3); // ease-out
        let v = from + ((to - from) as f64 * e).round() as i32;
        let (x, y) = if vertical { (v, rc.top) } else { (rc.left, v) };
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE);
        }
        std::thread::sleep(Duration::from_millis(14));
    }
}

/// One thread for the process lifetime; it idles unless auto-hide is on.
pub fn start() {
    std::thread::spawn(|| {
        let mut outside_since: Option<Instant> = None;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            if !super::AUTOHIDE.load(Ordering::Relaxed) || !super::is_visible() {
                outside_since = None;
                continue;
            }
            let h = super::dock_hwnd();
            if h.0.is_null() {
                continue;
            }
            let mon = super::primary_monitor();
            let mut rc = RECT::default();
            if unsafe { GetWindowRect(h, &mut rc) }.is_err() {
                continue;
            }
            let edge = super::edge();
            let vertical = super::vertical();
            let t = if vertical { rc.right - rc.left } else { rc.bottom - rc.top }; // thickness
            // Where the dock's near edge sits shown and hidden.
            let (shown, hidden) = match edge {
                super::Edge::Bottom => (mon.bottom - t, mon.bottom),
                super::Edge::Top => (mon.top, mon.top - t),
                super::Edge::Left => (mon.left, mon.left - t),
                super::Edge::Right => (mon.right - t, mon.right),
            };
            let mut p = POINT::default();
            let _ = unsafe { GetCursorPos(&mut p) };
            let (along, across) = if vertical { (p.y, p.x) } else { (p.x, p.y) };
            let (lo, hi) = if vertical { (mon.top, mon.bottom) } else { (mon.left, mon.right) };
            let in_x = along >= lo && along < hi;
            let at_edge = in_x
                && match edge {
                    super::Edge::Bottom => across >= mon.bottom - 1,
                    super::Edge::Top => across <= mon.top,
                    super::Edge::Left => across <= mon.left,
                    super::Edge::Right => across >= mon.right - 1,
                };
            let other = wanted_for_other_reasons();
            if super::HIDDEN.load(Ordering::Relaxed) {
                if at_edge || other {
                    super::HIDDEN.store(false, Ordering::Relaxed);
                    slide(h, if vertical { rc.left } else { rc.top }, shown, vertical);
                    log::info!("[autohide] dock shown");
                }
                outside_since = None;
            } else {
                let in_band = in_x
                    && match edge {
                        super::Edge::Bottom | super::Edge::Right => across >= shown - BAND_SLACK,
                        super::Edge::Top | super::Edge::Left => across <= shown + t + BAND_SLACK,
                    };
                if in_band || other {
                    outside_since = None;
                } else if outside_since.get_or_insert_with(Instant::now).elapsed() >= DWELL {
                    super::HIDDEN.store(true, Ordering::Relaxed);
                    slide(h, if vertical { rc.left } else { rc.top }, hidden, vertical);
                    outside_since = None;
                    log::info!("[autohide] dock hidden");
                }
            }
        }
    });
}
