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
mod claude_models;
mod claude_usage;
#[allow(dead_code)]
mod media;
#[allow(dead_code)]
mod system;
mod mixer;
mod screentime;
mod langpop;
mod nocaption;
mod update;
mod backup;
mod clip;
mod mic;
mod notes;
mod tip;
mod sensors;
mod hotkeys;
mod float;
mod brightness;
mod topapps;
mod settings;
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
    // After an update the previous process is still closing: wait for it instead of giving up.
    let after_update = std::env::args().any(|a| a == "--after-update");
    for _ in 0..if after_update { 40 } else { 1 } {
        // The handle of a successful claim is never closed: the mutex lives exactly as long as this process.
        let r = unsafe { CreateMutexW(None, false, windows::core::w!(r"Local\custom_taskbar_dock")) };
        let Ok(h) = r else { return false };
        if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
            return false;
        }
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(h); // do not keep the other process's mutex alive
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    true
}

/// One line in `startup.log` next to config.json. The release exe has no console and the log plugin starts late,
/// so this is the only trace of a launch that dies early (autostart at logon).
fn trace(msg: &str) {
    use std::io::Write;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(config::config_path().with_file_name("startup.log")) {
        let _ = writeln!(f, "{secs} pid {} {msg}", std::process::id());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    trace("launch");
    if already_running() {
        trace("already running, exiting");
        log::warn!("dock already running; exiting without touching the taskbar");
        return;
    }
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        trace(&format!("panic: {info}"));
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
        .on_window_event(|window, event| {
            if let (Some(id), tauri::WindowEvent::Moved(p)) = (window.label().strip_prefix("float-"), event) {
                float::moved(tauri::Manager::app_handle(window), id, p.x, p.y);
            }
            if let (Some(id), tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)) = (window.label().strip_prefix("note-"), event) {
                notes::geometry(tauri::Manager::app_handle(window), id);
            }
        })
        .setup(|app| {
            autostart::ensure();
            let hub = state::Hub::new(app.handle().clone());
            tauri::Manager::manage(app, hub);
            let cfg = config::Config::load(&config::config_path());
            apps::tracker::start(app.handle().clone(), apps::pins::effective());
            apps::pins::watch();
            battery::logitech::start(app.handle().clone(), cfg.poll_minutes.battery);
            weather::start(app.handle().clone(), cfg.weather, cfg.poll_minutes.weather);
            claude_usage::start(app.handle().clone(), cfg.poll_minutes.claude);
            screentime::start();
            sensors::start();
            hotkeys::start();
            topapps::start();
            media::start(app.handle().clone());
            system::start(app.handle().clone());
            tray::host::start(app.handle().clone());
            if let Err(e) = settings::tray_icon(app.handle()) {
                log::warn!("tray icon: {e}");
            }
            shell::setup(app.handle())?;
            trace("dock up");
            float::restore(app.handle());
            notes::restore(app.handle());
            clip::start(app.handle().clone());
            update::cleanup();
            update::start(app.handle().clone());
            let h = app.handle().clone();
            std::thread::spawn(move || {
                // Off the setup thread (building windows from the main thread's own closure deadlocks).
                std::thread::sleep(std::time::Duration::from_secs(3));
                mixer::warm(&h);
                tip::warm(&h);
                langpop::warm(&h);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![state::request_state, media::media_control,
            system::set_volume,
            system::toggle_mute,
            system::open_quick_settings,
            system::open_notifications,
            system::show_desktop, system::lang_get, system::lang_switch, langpop::lang_list, langpop::lang_pop_toggle, langpop::outdev_pop_toggle, langpop::app_menu_toggle, langpop::lang_select,
            shell::set_pill_bounds,
            apps::pins::app_click,
            apps::pins::start_menu,
            apps::preview::preview_hover, apps::preview::preview_leave, apps::preview::preview_enter,
            apps::preview::preview_items, apps::preview::activate_window, apps::preview::close_window,
            tray::host::tray_click,
            tray::host::open_tray_overflow,
            claude_usage::claude_refresh, screentime::screentime_get, sensors::sensors_get, sensors::battery_get, float::float_open, float::float_close, float::float_pin, topapps::top_get, brightness::brightness_get, brightness::brightness_set, topapps::open_task_manager,
            settings::get_settings, settings::set_layout, settings::set_theme,
            settings::set_bring_to_main, settings::set_widget_scale, settings::set_indicator, settings::set_start_icon, mic::mic_get, mic::mic_toggle, mic::bin_get, mic::bin_open, mic::bin_empty, notes::notes_count, notes::note_get, notes::note_new, notes::note_set, notes::note_flags, notes::note_delete, tip::tip_show, tip::tip_ready, tip::tip_hide, settings::set_dock, settings::set_autohide, settings::set_hover_lift, settings::set_divider_width, settings::set_icons, settings::set_auto_update, update::update_state, update::update_check, update::update_install, backup::backup_export, backup::backup_import, settings::set_look, clip::clip_count, clip::clip_clear, settings::autostart_get, settings::autostart_set, shell::monitors_list, settings::set_brightness_layout,
            settings::set_bring_key, settings::restart_app, settings::quit_app, settings::open_settings,
            mixer::mixer_list, mixer::mixer_set_volume, mixer::mixer_set_mute, mixer::mixer_toggle, mixer::mixer_fit
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                shell::teardown(); // give the work area and the Windows taskbar back
            }
        });
}
