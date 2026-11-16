#[allow(dead_code)] // consumers arrive in later tasks
mod config;
#[allow(dead_code)]
mod apps;
#[allow(dead_code)]
mod state;
#[allow(dead_code)]
mod battery;
#[allow(dead_code)]
mod weather;
#[allow(dead_code)]
mod claude_usage;
#[allow(dead_code)]
mod media;
#[allow(dead_code)]
mod system;
mod autostart;
mod shell;
mod tray;

/// Minimal stderr logger so `log::` output is visible in dev without a dependency.
struct StderrLog;
impl log::Log for StderrLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("{} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

fn init_log() {
    if log::set_logger(&StderrLog).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// `--guardian <pid>`: restore the Windows taskbar once the dock process exits, however it exits.
pub fn guardian(pid: u32) {
    init_log();
    shell::taskbar::guard(pid);
}

/// `--spawn-guardian <pid>`: start the guardian detached from the dock's process tree, then exit.
pub fn launch_guardian(pid: u32) {
    init_log();
    shell::taskbar::launch_guardian(pid);
}

/// True if another dock already runs in this session (autostart plus a manual launch).
fn already_running() -> bool {
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    // The handle is never closed: the mutex lives exactly as long as this process.
    let r = unsafe { CreateMutexW(None, false, windows::core::w!(r"Local\custom_taskbar_dock")) };
    r.is_ok() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if already_running() {
        log::warn!("dock already running; exiting without touching the taskbar");
        return;
    }
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        shell::teardown(); // never leave the user without a taskbar
        prev(info);
    }));
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
    let logs = config::config_path().with_file_name("logs");
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(1_000_000)
                .rotation_strategy(RotationStrategy::KeepSome(5))
                .clear_targets()
                .target(Target::new(TargetKind::Folder { path: logs, file_name: Some("dock".into()) }))
                .target(Target::new(TargetKind::Stderr))
                .build(),
        )
        .setup(|app| {
            autostart::ensure();
            let hub = state::Hub::new(app.handle().clone());
            tauri::Manager::manage(app, hub);
            let cfg = config::Config::load(&config::config_path());
            apps::tracker::start(app.handle().clone(), cfg.pins);
            battery::logitech::start(app.handle().clone(), cfg.poll_minutes.battery);
            weather::start(app.handle().clone(), cfg.weather, cfg.poll_minutes.weather);
            claude_usage::start(app.handle().clone(), cfg.poll_minutes.claude);
            media::start(app.handle().clone());
            system::start(app.handle().clone());
            tray::host::start(app.handle().clone());
            shell::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![state::request_state, media::media_control,
            system::set_volume,
            system::toggle_mute,
            system::open_quick_settings,
            system::open_notifications,
            shell::set_pill_bounds,
            apps::pins::app_click,
            apps::pins::start_menu,
            tray::host::tray_click,
            tray::host::open_tray_overflow
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                shell::teardown(); // give the work area and the Windows taskbar back
            }
        });
}
