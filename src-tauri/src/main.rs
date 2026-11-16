// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Guardian mode: no Tauri app, no window, no webview; just wait for the dock to die and restore.
    if args.get(1).map(String::as_str) == Some("--guardian") {
        custom_taskbar_lib::guardian(args.get(2).and_then(|p| p.parse().ok()).unwrap_or(0));
        return;
    }
    custom_taskbar_lib::run()
}
