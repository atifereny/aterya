#[allow(dead_code)] // consumers arrive in later tasks
mod config;
#[allow(dead_code)]
mod apps;
#[allow(dead_code)]
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let hub = state::Hub::new(app.handle().clone());
            tauri::Manager::manage(app, hub);
            let cfg = config::Config::load(&config::config_path());
            apps::tracker::start(app.handle().clone(), cfg.pins);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![state::request_state])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
