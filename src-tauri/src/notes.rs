//! Sticky notes: small frameless note windows kept on the desktop. Text, color, position and size are saved in
//! `notes.json` next to config.json; every saved note gets its window back at startup.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const MAX_NOTES: usize = 12;
const COLORS: [&str; 5] = ["sun", "ember", "teal", "rose", "lilac"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub text: String,
    pub color: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    /// Position and size are frozen.
    pub locked: bool,
    /// Stays above every other window.
    pub top: bool,
}

impl Default for Note {
    fn default() -> Self {
        Self { id: String::new(), text: String::new(), color: "sun".into(), x: 200, y: 200, w: 260, h: 260, locked: false, top: false }
    }
}

static NOTES: Mutex<Option<Vec<Note>>> = Mutex::new(None);

fn path() -> std::path::PathBuf {
    crate::config::config_path().with_file_name("notes.json")
}

fn with<R>(f: impl FnOnce(&mut Vec<Note>) -> R) -> R {
    let mut g = NOTES.lock().unwrap_or_else(|e| e.into_inner());
    let v = g.get_or_insert_with(|| std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default());
    f(v)
}

fn save(v: &[Note]) {
    if let Ok(t) = serde_json::to_string_pretty(v) {
        let p = path();
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, t).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

fn label(id: &str) -> String {
    format!("note-{id}")
}

async fn build(app: &AppHandle, n: &Note) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(&label(&n.id)) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, label(&n.id), WebviewUrl::App(format!("index.html#note={}", n.id).into()))
        .title("Not")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(n.top)
        .resizable(!n.locked)
        .inner_size(n.w as f64, n.h as f64)
        .min_inner_size(190.0, 150.0)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = w.set_position(PhysicalPosition::new(n.x, n.y));
    if let Ok(h) = w.hwnd() {
        let h = h.0 as isize;
        let _ = w.run_on_main_thread(move || crate::nocaption::apply(h));
    }
    Ok(w)
}

/// A window for every saved note. Called once at startup.
pub fn restore(app: &AppHandle) {
    let notes = with(|v| v.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for n in notes {
            if let Err(e) = build(&app, &n).await {
                log::warn!("[notes] {}: {e}", n.id);
            }
        }
    });
}

/// Every note, for the backup file.
pub fn all() -> Vec<Note> {
    with(|v| v.clone())
}

/// Replace all notes (backup import). Windows are rebuilt by `restore`.
pub fn replace(notes: Vec<Note>) {
    with(|v| {
        *v = notes.into_iter().filter(|n| !n.id.is_empty()).take(MAX_NOTES).collect();
        save(v);
    });
}

#[tauri::command]
pub fn notes_count() -> usize {
    with(|v| v.len())
}

#[tauri::command]
pub fn note_get(id: String) -> Option<Note> {
    with(|v| v.iter().find(|n| n.id == id).cloned())
}

/// A new, empty note in the middle of the primary screen, a little lower and to the right of the last one.
#[tauri::command]
pub async fn note_new(app: AppHandle) -> Result<(), String> {
    let count = with(|v| v.len());
    if count >= MAX_NOTES {
        return Err(format!("En fazla {MAX_NOTES} not olabilir"));
    }
    let mon = crate::shell::primary_monitor();
    let k = crate::shell::dock_k().max(1.0);
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis()).to_string();
    let (w, h) = ((260.0 * k) as i32, (260.0 * k) as i32);
    let off = (count as i32 % 6) * (34.0 * k) as i32;
    let n = Note {
        id,
        color: COLORS[count % COLORS.len()].into(),
        x: mon.left + (mon.right - mon.left - w) / 2 + off,
        y: mon.top + (mon.bottom - mon.top - h) / 2 + off,
        w: w as u32,
        h: h as u32,
        ..Default::default()
    };
    with(|v| {
        v.push(n.clone());
        save(v);
    });
    build(&app, &n).await.map(|_| ())
}

#[tauri::command]
pub fn note_set(id: String, text: String, color: String) {
    with(|v| {
        if let Some(n) = v.iter_mut().find(|n| n.id == id) {
            n.text = text;
            if COLORS.contains(&color.as_str()) {
                n.color = color;
            }
            save(v);
        }
    });
}

/// Lock a note in place and/or keep it above every other window.
#[tauri::command]
pub fn note_flags(app: AppHandle, id: String, locked: bool, top: bool) {
    with(|v| {
        if let Some(n) = v.iter_mut().find(|n| n.id == id) {
            (n.locked, n.top) = (locked, top);
            save(v);
        }
    });
    if let Some(w) = app.get_webview_window(&label(&id)) {
        let _ = w.set_always_on_top(top);
        let _ = w.set_resizable(!locked);
    }
}

#[tauri::command]
pub fn note_delete(app: AppHandle, id: String) {
    with(|v| {
        v.retain(|n| n.id != id);
        save(v);
    });
    if let Some(w) = app.get_webview_window(&label(&id)) {
        let _ = w.close();
    }
}

static GEN: AtomicU64 = AtomicU64::new(0);

/// A note window moved or was resized: save its geometry once it has been still for 300 ms.
pub fn geometry(app: &AppHandle, id: &str) {
    let me = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let (app, id) = (app.clone(), id.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if GEN.load(Ordering::SeqCst) != me {
            return;
        }
        let Some(w) = app.get_webview_window(&label(&id)) else { return };
        let (Ok(p), Ok(s)) = (w.outer_position(), w.inner_size()) else { return };
        with(|v| {
            if let Some(n) = v.iter_mut().find(|n| n.id == id) {
                if (n.x, n.y, n.w, n.h) != (p.x, p.y, s.width, s.height) {
                    (n.x, n.y, n.w, n.h) = (p.x, p.y, s.width, s.height);
                    save(v);
                }
            }
        });
    });
}
