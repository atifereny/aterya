//! Hide the Windows taskbar while the dock runs, and always give it back.
//! Invariant: auto-hide is only changed by us while taskbar-state.json holds the original value.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindowThreadProcessId, EVENT_OBJECT_SHOW, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_GETSTATE, ABM_SETSTATE, ABS_ALWAYSONTOP, ABS_AUTOHIDE, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, ShowWindow, SW_HIDE, SW_SHOWNA};

/// Where the pre-dock auto-hide setting is kept, so a guardian or the next start can restore it.
pub fn state_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("custom_taskbar").join("taskbar-state.json")
}

fn save_original(path: &Path, autohide: bool) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::json!({ "autohide": autohide }).to_string())
}

/// None: nothing saved (taskbar untouched). A corrupt file reads as auto-hide off, the safe side.
fn load_original(path: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    Some(v["autohide"].as_bool().unwrap_or(false))
}

/// Shell_TrayWnd first, then every Shell_SecondaryTrayWnd.
fn taskbars() -> Vec<HWND> {
    let mut v: Vec<HWND> = unsafe { FindWindowExW(None, None, w!("Shell_TrayWnd"), PCWSTR::null()) }.into_iter().collect();
    let mut after = None;
    while let Ok(h) = unsafe { FindWindowExW(None, after, w!("Shell_SecondaryTrayWnd"), PCWSTR::null()) } {
        v.push(h);
        after = Some(h);
    }
    v
}

fn appbar_state(msg: u32, tray: HWND, state: u32) -> u32 {
    let mut d = APPBARDATA { cbSize: size_of::<APPBARDATA>() as u32, hWnd: tray, lParam: LPARAM(state as isize), ..Default::default() };
    unsafe { SHAppBarMessage(msg, &mut d) as u32 }
}

/// True between hide_all and restore_all in this process: taskbars that explorer re-shows get hidden again.
static HIDING: AtomicBool = AtomicBool::new(false);
/// Explorer pid the show hook watches (a new explorer after a restart needs a new hook).
static HOOKED_PID: AtomicU32 = AtomicU32::new(0);

fn is_taskbar(h: HWND) -> bool {
    let mut buf = [0u16; 32];
    let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
    matches!(String::from_utf16_lossy(&buf[..n]).as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

unsafe extern "system" fn on_show(_: HWINEVENTHOOK, _: u32, h: HWND, obj: i32, _: i32, _: u32, _: u32) {
    if obj == OBJID_WINDOW.0 && HIDING.load(Ordering::SeqCst) && is_taskbar(h) {
        unsafe { let _ = ShowWindow(h, SW_HIDE); }
    }
}

/// Explorer re-shows the tray after an auto-hide change and on edge hover; watch its show events.
/// Must run on a thread with a message loop (the main thread).
// shortcut: the hook on a dead explorer is never unhooked (one tiny leak per Explorer restart), fine
// unless Explorer restarts hundreds of times per session.
fn watch_explorer(tray: HWND) {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(tray, Some(&mut pid)) };
    if pid == 0 || HOOKED_PID.swap(pid, Ordering::SeqCst) == pid {
        return;
    }
    let hook = unsafe { SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, None, Some(on_show), pid, 0, WINEVENT_OUTOFCONTEXT) };
    if hook.is_invalid() {
        log::error!("[taskbar] show hook failed; explorer may bring the taskbar back over the dock");
    }
}

/// Record the original auto-hide (once), turn auto-hide on so the work area is freed, hide every taskbar.
/// Idempotent; re-run after an Explorer restart.
pub fn hide_all() {
    let bars = taskbars();
    let Some(&tray) = bars.first() else {
        log::warn!("[taskbar] no Shell_TrayWnd (explorer down?), nothing to hide");
        return;
    };
    let path = state_path();
    let saved = load_original(&path).is_some()
        || match save_original(&path, appbar_state(ABM_GETSTATE, tray, 0) & ABS_AUTOHIDE != 0) {
            Ok(()) => true,
            Err(e) => {
                log::error!("[taskbar] cannot save original state to {}: {e}; leaving auto-hide alone", path.display());
                false
            }
        };
    HIDING.store(true, Ordering::SeqCst);
    watch_explorer(tray);
    if saved {
        appbar_state(ABM_SETSTATE, tray, ABS_AUTOHIDE);
    }
    for h in &bars {
        unsafe { let _ = ShowWindow(*h, SW_HIDE); }
    }
    log::info!("[taskbar] hid {} taskbar window(s)", bars.len());
}

/// Show every taskbar and put auto-hide back to the saved original. Idempotent; safe from any
/// process (the guardian calls it after the dock dies).
pub fn restore_all() {
    HIDING.store(false, Ordering::SeqCst);
    let bars = taskbars();
    let path = state_path();
    if let (Some(&tray), Some(autohide)) = (bars.first(), load_original(&path)) {
        appbar_state(ABM_SETSTATE, tray, if autohide { ABS_AUTOHIDE } else { ABS_ALWAYSONTOP });
        let _ = std::fs::remove_file(&path);
    }
    for h in &bars {
        unsafe { let _ = ShowWindow(*h, SW_SHOWNA); }
    }
    log::info!("[taskbar] restored {} taskbar window(s)", bars.len());
}

/// Launch `<this exe> <mode> <pid>` detached: no console, out of our job if the job allows it.
fn spawn_self(mode: &str, pid: u32) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let exe = std::env::current_exe()?;
    let run = |flags| std::process::Command::new(&exe).args([mode, &pid.to_string()]).creation_flags(flags).spawn().map(drop);
    // Breakaway is denied (access denied) inside jobs without JOB_OBJECT_LIMIT_BREAKAWAY_OK.
    run(DETACHED_PROCESS | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB).or_else(|_| run(DETACHED_PROCESS | CREATE_NO_WINDOW))
}

/// Start the guardian for this process. It goes through a short-lived launcher
/// (`--spawn-guardian`) so the guardian's parent is dead: "End process tree" / `taskkill /T`
/// walk parent pids and so cannot reach it.
pub fn spawn_guardian() {
    if let Err(e) = spawn_self("--spawn-guardian", std::process::id()) {
        log::error!("[taskbar] guardian launcher not started: {e}");
    }
}

/// Launcher mode: start the real guardian for `pid`, then return (the launcher exits at once).
pub fn launch_guardian(pid: u32) {
    if let Err(e) = spawn_self("--guardian", pid) {
        log::error!("[taskbar] guardian not started: {e}");
    }
}

/// Guardian mode: wait for `pid` to exit (however it dies), restore, return. No window, no webview.
pub fn guard(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};
    // OpenProcess failing means the parent is already gone: restore straight away.
    if let Ok(h) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        unsafe {
            WaitForSingleObject(h, INFINITE);
            let _ = CloseHandle(h);
        }
    }
    restore_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ctb_tb_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&d);
        d.join("taskbar-state.json")
    }

    #[test]
    fn original_state_round_trips() {
        for v in [true, false] {
            let p = tmp(&format!("rt{v}"));
            save_original(&p, v).unwrap();
            assert_eq!(load_original(&p), Some(v));
        }
    }

    #[test]
    fn missing_file_means_untouched() {
        assert_eq!(load_original(&tmp("missing")), None);
    }

    #[test]
    fn corrupt_file_restores_autohide_off() {
        let p = tmp("corrupt");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(load_original(&p), Some(false));
    }
}
