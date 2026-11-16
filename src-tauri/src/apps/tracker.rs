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
}

struct Ctx {
    app: AppHandle,
    pins: Vec<Pin>,
}

static CTX: OnceLock<Ctx> = OnceLock::new();
/// Top-level windows, most recently foregrounded first.
static RECENT: Mutex<Vec<isize>> = Mutex::new(Vec::new());
/// Stable button order for unpinned running apps (lowercased ids, first-seen order).
static ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());
static GROUPS: Mutex<Option<HashMap<String, GroupState>>> = Mutex::new(None);
static LAST: Mutex<Vec<AppEntry>> = Mutex::new(Vec::new());

/// Group state for a button id (pin id or group id), as of the last refresh.
pub fn group_state(id: &str) -> Option<GroupState> {
    GROUPS.lock().unwrap().as_ref()?.get(&id.to_lowercase()).cloned()
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
    if CTX.set(Ctx { app, pins }).is_err() {
        return;
    }
    std::thread::spawn(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        GATE.with_borrow_mut(Gate::startup);
        run_refresh(false);
        // create..hide, namechange, foreground, minimizestart..end
        for (lo, hi) in [(0x8000, 0x8003), (0x800C, 0x800C), (0x0003, 0x0003), (0x0016, 0x0017)] {
            SetWinEventHook(lo, hi, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT);
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
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
unsafe fn exe_of(h: HWND, own_pid: u32) -> Option<String> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == 0 || pid == own_pid {
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
        let Some(exe) = (unsafe { exe_of(h, own_pid) }) else { continue };
        // shortcut: AUMID-first grouping can split a Win32 app that sets an explicit AUMID from its
        // exe-path pin, upgrade by mapping the AUMID back to its exe when that shows up.
        let aumid = unsafe { aumid_of(h) };
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

    let fg = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) }.0 as isize;
    let mut recent = RECENT.lock().unwrap();
    recent.retain(|w| found.values().any(|(_, f)| f.wins.iter().any(|x| x.hwnd == *w)));
    let rank = |w: &Win| recent.iter().position(|&r| r == w.hwnd).unwrap_or(usize::MAX);

    let mut order = ORDER.lock().unwrap();
    let pin_keys: Vec<String> = ctx.pins.iter().map(|p| p.id.to_lowercase()).collect();
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
        let pin_id = ctx.pins.iter().find(|p| p.id.to_lowercase() == key).map(|p| p.id.clone());
        let (id, name, icon_id, mut wins) = match found.remove(&key) {
            Some((id, f)) => {
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
    use super::Gate;

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
