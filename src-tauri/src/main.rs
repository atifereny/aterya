// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Guardian mode: no Tauri app, no window, no webview; just wait for the dock to die and restore.
    let pid = || args.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
    match args.get(1).map(String::as_str) {
        Some("--guardian") => return custom_taskbar_lib::guardian(pid()),
        Some("--spawn-guardian") => return custom_taskbar_lib::launch_guardian(pid()),
        _ => {}
    }
    custom_taskbar_lib::run()
}
