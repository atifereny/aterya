//! Floating widgets: a widget taken out of the dock into its own small always-on-top window.
//! The window is moved by hand (drag handle), the position is saved, and a pin locks it in place.
use crate::config::{FloatCfg, WIDGETS};
use crate::settings::{current, save_and_emit};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

fn label(id: &str) -> String {
    format!("float-{id}")
}

/// Builds the window; call from an async context (building on the main thread deadlocks).
async fn build(app: &AppHandle, id: &str, at: Option<(i32, i32)>) -> Result<(), String> {
    if app.get_webview_window(&label(id)).is_some() {
        return Ok(());
    }
    let w = WebviewWindowBuilder::new(app, label(id), WebviewUrl::App(format!("index.html#float={id}").into()))
        .title("Aterya widget")
        .inner_size(280.0, 110.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .skip_taskbar(true)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    if let Ok(h) = w.hwnd() {
        let h = h.0 as isize;
        let _ = w.run_on_main_thread(move || crate::nocaption::apply(h));
    }
    if let Some((x, y)) = at {
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    Ok(())
}

/// Windows for every saved floating widget, at their saved spots. Called once at startup.
pub fn restore(app: &AppHandle) {
    let cfg = match current() {
        Ok(c) => c,
        Err(e) => return log::warn!("[float] config unreadable: {e}"),
    };
    log::info!("[float] restoring {} window(s)", cfg.floating.len());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for f in cfg.floating {
            match build(&app, &f.id, Some((f.x, f.y))).await {
                Ok(()) => log::info!("[float] {} restored at {},{}", f.id, f.x, f.y),
                Err(e) => log::warn!("[float] {}: {e}", f.id),
            }
        }
    });
}

/// Take a widget out of the dock: it appears in the middle of the screen, ready to be moved.
#[tauri::command]
pub async fn float_open(app: AppHandle, id: String) -> Result<(), String> {
    if id == "apps" || !WIDGETS.contains(&id.as_str()) {
        return Err("bu widget masaüstüne alınamaz".into());
    }
    let mut c = current()?;
    if c.floating.iter().any(|f| f.id == id) {
        return Ok(());
    }
    build(&app, &id, None).await?;
    let pos = app.get_webview_window(&label(&id)).and_then(|w| w.outer_position().ok()).map(|p| (p.x, p.y)).unwrap_or((0, 0));
    c.floating.push(FloatCfg { id, x: pos.0, y: pos.1, pinned: false });
    save_and_emit(&app, &c)
}

/// Put the widget back into the dock.
#[tauri::command]
pub async fn float_close(app: AppHandle, id: String) -> Result<(), String> {
    let mut c = current()?;
    c.floating.retain(|f| f.id != id);
    if let Some(w) = app.get_webview_window(&label(&id)) {
        let _ = w.close();
    }
    save_and_emit(&app, &c)
}

#[tauri::command]
pub fn float_pin(app: AppHandle, id: String, pinned: bool) -> Result<(), String> {
    let mut c = current()?;
    if let Some(f) = c.floating.iter_mut().find(|f| f.id == id) {
        f.pinned = pinned;
    }
    save_and_emit(&app, &c)
}

static GEN: AtomicU64 = AtomicU64::new(0);

/// A floating window moved: save the spot once the drag has been still for 250 ms.
pub fn moved(app: &AppHandle, id: &str, x: i32, y: i32) {
    let me = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let (app, id) = (app.clone(), id.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if GEN.load(Ordering::SeqCst) != me {
            return;
        }
        let Ok(mut c) = current() else { return };
        if let Some(f) = c.floating.iter_mut().find(|f| f.id == id && !f.pinned && (f.x, f.y) != (x, y)) {
            (f.x, f.y) = (x, y);
            let _ = save_and_emit(&app, &c);
        }
    });
}
