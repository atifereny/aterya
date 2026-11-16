use serde::Serialize;
use std::{collections::HashMap, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Emits events to the frontend and caches the last payload of each, so a
/// late-loading webview can ask for everything again via `request_state`.
pub struct Hub<R: Runtime = tauri::Wry> {
    app: AppHandle<R>,
    cache: Mutex<HashMap<String, serde_json::Value>>,
}

impl<R: Runtime> Hub<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app, cache: Mutex::default() }
    }

    pub fn emit<T: Serialize + Clone>(&self, event: &str, payload: T) {
        if let Ok(v) = serde_json::to_value(&payload) {
            self.cache.lock().unwrap().insert(event.to_string(), v);
        }
        let _ = self.app.emit(event, payload);
    }

    pub fn replay(&self) {
        for (event, v) in self.cache.lock().unwrap().iter() {
            let _ = self.app.emit(event, v);
        }
    }
}

#[tauri::command]
pub fn request_state(app: AppHandle) {
    app.state::<Hub>().replay();
}
