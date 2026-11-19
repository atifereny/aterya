use super::{
    click::GroupState,
    filter::{WindowInfo, is_taskbar_window},
    icons,
};
use crate::{config::Pin, state::Hub};
use serde::Serialize;
use std::{collections::HashMap, path::Path, sync::{Mutex, OnceLock}};
use tauri::{AppHandle, Manager};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, TRUE},
        Storage::EnhancedStorage::PKEY_AppUserModel_ID,
        Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
        System::{
            Com::{StructuredStorage::PropVariantToStringAlloc, COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree},
            Threading::{
                GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook},
            Input::KeyboardAndMouse::{KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_MENU, keybd_event},
            Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, PWSTR},
};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub icon_data_url: String,
    pub pinned: bool,
    pub running: bool,
    pub focused: bool,
    pub window_count: u32,
    /// Unread count read from the window titles, e.g. "(3) Discord"; 0 when none.
    pub badge: u32,
    /// A window of the app is flashing for attention.
    pub attention: bool,
}

struct Ctx {
    app: AppHandle,
}

/// Pins in dock order; replaced live when the user pins or unpins (or the Windows taskbar's pins change).
static PINS: Mutex<Vec<Pin>> = Mutex::new(Vec::new());
/// Windows that asked for attention (FlashWindow), until they are activated or closed.
static FLASH: Mutex<std::collections::BTreeSet<isize>> = Mutex::new(std::collections::BTreeSet::new());
static SHELLHOOK_MSG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static HOOK_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static THREAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
const WM_PINS: u32 = WM_APP + 7;

/// Unread count in a window title: "(3) Name", "[3] Name" or "Name (3)". 0 when there is none.
fn title_badge(t: &str) -> u32 {
    let t = t.trim();
    let num = |s: &str| s.parse::<u32>().ok().filter(|n| (1..=999).contains(n));
    if let Some(r) = t.strip_prefix('(').or_else(|| t.strip_prefix('[')) {
        if let Some(end) = r.find([')', ']']) {
            if let Some(n) = num(&r[..end]) {
                return n;
            }
        }
    }
    if t.ends_with(')') || t.ends_with(']') {
        if let Some(i) = t.rfind(['(', '[']) {
            if let Some(n) = num(&t[i + 1..t.len() - 1]) {
                return n;
            }
        }
    }
    0
}

/// Browsers put page and extension counters in their titles; those are not unread messages.
fn is_browser(exe: &str) -> bool {
    let stem = stem(exe).to_lowercase();
    ["chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "arc", "chromium", "iexplore", "zen", "helium", "thorium", "librewolf", "waterfox"].contains(&stem.as_str())
}

/// A hidden window that receives the shell's hook messages (flashing windows, activation).
unsafe fn shell_hook() {
    use windows::core::w;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    unsafe {
        let Ok(hinst) = GetModuleHandleW(None) else { return };
        let class = w!("AteryaShellHook");
        let wc = WNDCLASSW { lpfnWndProc: Some(hook_proc), hInstance: hinst.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        let Ok(h) = CreateWindowExW(WS_EX_TOOLWINDOW, class, w!(""), WS_POPUP, 0, 0, 0, 0, None, None, Some(hinst.into()), None) else { return };
        SHELLHOOK_MSG.store(RegisterWindowMessageW(w!("SHELLHOOK")), std::sync::atomic::Ordering::Relaxed);
        let _ = RegisterShellHookWindow(h);
        HOOK_HWND.store(h.0 as isize, std::sync::atomic::Ordering::Relaxed);
    }
}

unsafe extern "system" fn hook_proc(h: HWND, msg: u32, wp: windows::Win32::Foundation::WPARAM, lp: LPARAM) -> windows::Win32::Foundation::LRESULT {
    if msg == WM_PINS {
        if GATE.with_borrow_mut(Gate::mark) {
            unsafe { arm_timer() };
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == SHELLHOOK_MSG.load(std::sync::atomic::Ordering::Relaxed) && msg != 0 {
        let changed = match wp.0 as u32 {
            0x8006 => FLASH.lock().unwrap().insert(lp.0), // HSHELL_FLASH
            0x0004 | 0x8004 => FLASH.lock().unwrap().remove(&lp.0), // window activated
            _ => false,
        };
        if changed && GATE.with_borrow_mut(Gate::mark) {
            unsafe { arm_timer() };
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    unsafe { DefWindowProcW(h, msg, wp, lp) }
}

/// Replace the pins and refresh the row.
pub fn set_pins(pins: Vec<Pin>) {
    *PINS.lock().unwrap() = pins;
    // A window message, not a thread message: nested COM loops inside refresh() would swallow the latter.
    let h = HOOK_HWND.load(std::sync::atomic::Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_PINS, windows::Win32::Foundation::WPARAM(0), LPARAM(0));
        }
    }
}

/// The dock entry with this id, as of the last refresh.
pub fn entry(id: &str) -> Option<AppEntry> {
    LAST.lock().unwrap().iter().find(|e| e.id.eq_ignore_ascii_case(id)).cloned()
}

static CTX: OnceLock<Ctx> = OnceLock::new();
/// Top-level windows, most recently foregrounded first.
static RECENT: Mutex<Vec<isize>> = Mutex::new(Vec::new());
/// Stable button order for unpinned running apps (lowercased ids, first-seen order).
static ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());
static GROUPS: Mutex<Option<HashMap<String, GroupState>>> = Mutex::new(None);
static LAST: Mutex<Vec<AppEntry>> = Mutex::new(Vec::new());

/// Group state for a button id (pin id or group id), as of the last refresh.
/// The nth app in dock order (Win+1 is index 0).
pub fn nth_id(n: usize) -> Option<String> {
    LAST.lock().unwrap().get(n).map(|e| e.id.clone())
}

pub fn group_state(id: &str) -> Option<GroupState> {
    GROUPS.lock().unwrap().as_ref()?.get(&id.to_lowercase()).cloned()
}

/// Icon data URL of a button id, as of the last refresh.
pub fn icon_of(id: &str) -> String {
    LAST.lock().unwrap().iter().find(|e| e.id.eq_ignore_ascii_case(id)).map(|e| e.icon_data_url.clone()).unwrap_or_default()
}

/// Bring a window to the front, restoring it first if minimized.
pub fn activate(hwnd: isize) {
    let h = HWND(hwnd as *mut _);
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        // Windows refuses SetForegroundWindow from background processes; an Alt tap unlocks it.
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        keybd_event(VK_MENU.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        let _ = SetForegroundWindow(h);
    }
}

pub fn minimize(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(HWND(hwnd as *mut _), SW_MINIMIZE);
    }
}

/// Spawns the tracker thread: initial enumeration, then WinEvent hooks drive refreshes.
// shortcut: pins are read once at start so config edits need a restart, upgrade when pin editing lands.
pub fn start(app: AppHandle, pins: Vec<Pin>) {
    *PINS.lock().unwrap() = pins;
    if CTX.set(Ctx { app }).is_err() {
        return;
    }
    std::thread::spawn(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        THREAD.store(windows::Win32::System::Threading::GetCurrentThreadId(), std::sync::atomic::Ordering::Relaxed);
        GATE.with_borrow_mut(Gate::startup);
        shell_hook();
        run_refresh(false);
        // create..hide, namechange, foreground, minimizestart..end
        for (lo, hi) in [(0x8000, 0x8003), (0x800C, 0x800C), (0x0003, 0x0003), (0x0016, 0x0017)] {
            SetWinEventHook(lo, hi, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT);
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.hwnd.0.is_null() && msg.message == WM_PINS {
                GATE.with_borrow_mut(Gate::startup); // marks dirty
                run_refresh(false);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

/// Event coalescing and reentrancy state (hook thread only). Pure so it can be unit tested.
#[derive(Default)]
pub(crate) struct Gate {
    busy: bool,
    dirty: bool,
    timer_armed: bool,
}

impl Gate {
    /// An event arrived. Returns true when the caller must arm the one-shot timer.
    pub fn mark(&mut self) -> bool {
        self.dirty = true;
        !std::mem::replace(&mut self.timer_armed, true)
    }
    /// The timer fired. Returns true when a refresh should run now.
    pub fn on_timer(&mut self) -> bool {
        self.timer_armed = false;
        self.begin()
    }
    /// Request the initial refresh without touching the timer.
    pub fn startup(&mut self) {
        self.dirty = true;
    }
    pub fn begin(&mut self) -> bool {
        if self.busy || !self.dirty {
            return false;
        }
        (self.busy, self.dirty) = (true, false);
        true
    }
    /// Refresh done. Returns true when events arrived meanwhile and the timer must be armed.
    pub fn finish(&mut self) -> bool {
        self.busy = false;
        self.dirty && !std::mem::replace(&mut self.timer_armed, true)
    }
}

thread_local!(static GATE: std::cell::RefCell<Gate> = Default::default());

const DEBOUNCE_MS: u32 = 100;

unsafe fn arm_timer() {
    unsafe { SetTimer(None, 0, DEBOUNCE_MS, Some(on_timer)) };
}

unsafe fn run_refresh(from_timer: bool) {
    unsafe {
        let go = GATE.with_borrow_mut(|g| if from_timer { g.on_timer() } else { g.begin() });
        if go {
            refresh();
            if GATE.with_borrow_mut(Gate::finish) {
                arm_timer();
            }
        }
    }
}

/// One-shot: kills itself, then refreshes once. A timer proc still fires inside nested COM loops.
unsafe extern "system" fn on_timer(_: HWND, _: u32, id: usize, _: u32) {
    unsafe {
        let _ = KillTimer(None, id);
        run_refresh(true);
    }
}

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, event: u32, hwnd: HWND, id_object: i32, id_child: i32, _: u32, _: u32) {
    if id_object != OBJID_WINDOW.0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    unsafe {
        // Null root means the hwnd is already gone (late destroy/hide): still refresh.
        let root = GetAncestor(hwnd, GA_ROOT);
        if !root.0.is_null() && root != hwnd {
            return; // child windows, tooltips etc. never get a button
        }
        if event == EVENT_SYSTEM_FOREGROUND && !root.0.is_null() {
            let root = root.0 as isize;
            let mut r = RECENT.lock().unwrap();
            r.retain(|&w| w != root);
            r.insert(0, root);
        }
        if GATE.with_borrow_mut(Gate::mark) {
            arm_timer();
        }
    }
}

struct Win {
    hwnd: isize,
    title: String,
}

struct Found {
    exe: String,
    aumid: Option<String>,
    wins: Vec<Win>,
}

unsafe extern "system" fn collect(h: HWND, l: LPARAM) -> BOOL {
    unsafe { (*(l.0 as *mut Vec<HWND>)).push(h) };
    TRUE
}

unsafe fn info(h: HWND) -> WindowInfo {
    unsafe {
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(h, &mut buf);
        WindowInfo {
            visible: IsWindowVisible(h).as_bool(),
            has_owner: GetWindow(h, GW_OWNER).is_ok_and(|o| !o.0.is_null()),
            tool_window: ex & WS_EX_TOOLWINDOW.0 != 0,
            app_window: ex & WS_EX_APPWINDOW.0 != 0,
            cloaked: cloaked != 0,
            title: String::from_utf16_lossy(&buf[..n as usize]),
        }
    }
}

/// Exe path of the window's process; None for our own windows or inaccessible processes.
unsafe fn exe_of(h: HWND, own_pid: u32, allow_own: bool) -> Option<String> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == 0 || (pid == own_pid && !allow_own) {
            return None;
        }
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize * 2];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(p);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

unsafe fn aumid_of(h: HWND) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(h).ok()?;
        let pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&pv as *const _).ok()?;
        let out = s.to_string().ok();
        CoTaskMemFree(Some(s.0 as *const _));
        out.filter(|s| !s.is_empty())
    }
}

fn stem(path: &str) -> String {
    Path::new(path).file_stem().map_or(path.into(), |s| s.to_string_lossy().into_owned())
}

unsafe fn refresh() {
    let Some(ctx) = CTX.get() else { return };
    let own_pid = unsafe { GetCurrentProcessId() };
    let pins = PINS.lock().unwrap().clone();
    // Our own windows stay out of the row, except the settings window (identified by its handle, not its title).
    let settings_hwnd = ctx.app.get_webview_window("settings").and_then(|w| w.hwnd().ok()).map_or(0, |h| h.0 as isize);
    let mut hwnds = Vec::<HWND>::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut hwnds as *mut _ as isize));
    }

    // lowercased group id -> (display id, group)
    let mut found: HashMap<String, (String, Found)> = HashMap::new();
    let mut seen_order = Vec::new();
    for h in hwnds {
        let i = unsafe { info(h) };
        if !is_taskbar_window(&i) {
            continue;
        }
        // Our own windows stay out of the row, except the settings window (it is a real app window).
        let Some(exe) = (unsafe { exe_of(h, own_pid, h.0 as isize == settings_hwnd) }) else {
            continue;
        };
        // shortcut: AUMID-first grouping can split a Win32 app that sets an explicit AUMID from its
        // exe-path pin, upgrade by mapping the AUMID back to its exe when that shows up.
        // Explorer windows set no AUMID; group them under the id of the taskbar's File Explorer pin.
        let aumid = unsafe { aumid_of(h) }
            .or_else(|| exe.to_lowercase().ends_with(r"\explorer.exe").then(|| "Microsoft.Windows.Explorer".to_string()));
        let id = aumid.as_ref().map_or(exe.clone(), |a| format!("aumid:{a}"));
        let key = id.to_lowercase();
        if !found.contains_key(&key) {
            seen_order.push(key.clone());
        }
        found
            .entry(key)
            .or_insert_with(|| (id, Found { exe, aumid, wins: vec![] }))
            .1
            .wins
            .push(Win { hwnd: h.0 as isize, title: i.title });
    }

    let flash: std::collections::BTreeSet<isize> = {
        let mut f = FLASH.lock().unwrap();
        f.retain(|w| unsafe { IsWindow(Some(HWND(*w as *mut _))) }.as_bool());
        f.clone()
    };
    let fg = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) }.0 as isize;
    let mut recent = RECENT.lock().unwrap();
    recent.retain(|w| found.values().any(|(_, f)| f.wins.iter().any(|x| x.hwnd == *w)));
    let rank = |w: &Win| recent.iter().position(|&r| r == w.hwnd).unwrap_or(usize::MAX);

    let mut order = ORDER.lock().unwrap();
    let pin_keys: Vec<String> = pins.iter().map(|p| p.id.to_lowercase()).collect();
    order.retain(|k| found.contains_key(k) && !pin_keys.contains(k));
    for k in seen_order {
        if !order.contains(&k) && !pin_keys.contains(&k) {
            order.push(k);
        }
    }
    let keys: Vec<(String, bool)> =
        pin_keys.iter().cloned().map(|k| (k, true)).chain(order.iter().cloned().map(|k| (k, false))).collect();

    let mut entries = Vec::new();
    let mut pending = Vec::new();
    let mut groups = HashMap::new();
    for (key, pinned) in keys {
        let pin_id = pins.iter().find(|p| p.id.to_lowercase() == key).map(|p| p.id.clone());
        let mut browser = false;
        let (id, name, icon_id, mut wins) = match found.remove(&key) {
            Some((id, f)) => {
                browser = is_browser(&f.exe);
                let (name, icon_id) =
                    if f.aumid.is_some() { (f.wins[0].title.clone(), id.clone()) } else { (stem(&f.exe), f.exe.clone()) };
                (id, name, icon_id, f.wins)
            }
            None if pinned => {
                let id = pin_id.clone().unwrap();
                let name = match id.strip_prefix("aumid:") {
                    Some(a) => a.rsplit(['.', '!']).next().unwrap_or(a).to_string(),
                    None => stem(&id),
                };
                (id.clone(), name, id, vec![])
            }
            None => continue,
        };
        wins.sort_by_key(|w| rank(w));
        let state = GroupState {
            focused: wins.iter().find(|w| w.hwnd == fg).map(|w| w.hwnd),
            windows: wins.iter().map(|w| w.hwnd).collect(),
        };
        pending.push((icon_id, state.windows.first().copied()));
        entries.push(AppEntry {
            icon_data_url: String::new(),
            name,
            pinned,
            running: !wins.is_empty(),
            focused: state.focused.is_some(),
            window_count: wins.len() as u32,
            badge: if browser { 0 } else { wins.iter().map(|w| title_badge(&w.title)).sum::<u32>().min(999) },
            attention: wins.iter().any(|w| flash.contains(&w.hwnd)),
            id: pin_id.unwrap_or(id),
        });
        groups.insert(key, state);
    }
    drop((recent, order));
    // Shell COM calls pump messages, so no lock may be held here.
    for (e, (icon_id, hwnd)) in entries.iter_mut().zip(pending) {
        e.icon_data_url = icons::icon_for(&icon_id, hwnd);
    }

    *GROUPS.lock().unwrap() = Some(groups);
    let mut last = LAST.lock().unwrap();
    if *last != entries {
        *last = entries.clone();
        ctx.app.state::<Hub>().emit("apps://changed", entries);
    }
}

#[cfg(test)]
mod tests {
    use super::{title_badge, Gate};

    #[test]
    fn badges_from_titles() {
        assert_eq!(title_badge("(3) Discord"), 3);
        assert_eq!(title_badge("Telegram (12)"), 12);
        assert_eq!(title_badge("[5] Mail"), 5);
        assert_eq!(title_badge("Discord"), 0);
        assert_eq!(title_badge("Report (draft)"), 0);
        assert_eq!(title_badge("(0) Chat"), 0);
        assert_eq!(title_badge("Plan (2024) final"), 0);
    }


    #[test]
    fn events_coalesce_into_one_timer() {
        let mut g = Gate::default();
        assert!(g.mark());
        assert!(!g.mark());
        assert!(g.on_timer());
        assert!(!g.on_timer()); // nothing dirty
    }

    #[test]
    fn startup_leaves_timer_free_for_later_events() {
        let mut g = Gate::default();
        g.startup();
        assert!(g.begin());
        assert!(!g.finish());
        assert!(g.mark()); // a later event must arm the timer
    }

    #[test]
    fn event_during_refresh_defers_and_reruns() {
        let mut g = Gate::default();
        g.mark();
        assert!(g.on_timer()); // refresh running
        assert!(g.mark()); // nested event re-arms
        assert!(!g.on_timer()); // nested timer while busy: no reentry
        assert!(g.finish()); // dirty left over: arm again
        assert!(g.on_timer());
        assert!(!g.finish());
    }
}
