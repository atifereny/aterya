//! Dock settings: widget order/visibility and theme, stored in config.json and applied live.
use crate::config::{self, Config, FloatCfg, LayoutCfg, ThemeCfg};
use serde::Serialize;

// camelCase for the frontend, like Config.
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub layout: LayoutCfg,
    pub theme: ThemeCfg,
    pub bring_to_main: bool,
    pub bring_key: String,
    pub floating: Vec<FloatCfg>,
    pub scales: std::collections::HashMap<String, f64>,
    pub brightness_layout: String,
    pub indicator: String,
    pub start_icon: String,
    pub dock_monitor: String,
    pub dock_scale: f64,
    pub dock_gap: f64,
    pub dock_autohide: bool,
    pub hover_lift: bool,
    pub divider_width: f64,
    pub icon_size: f64,
    pub icon_gap: f64,
    pub dock_radius: f64,
    pub dock_border: f64,
    pub dock_text: String,
    pub dock_accent: String,
}

pub(crate) fn dock_scale(c: &Config) -> f64 {
    if c.dock_scale.is_finite() && c.dock_scale > 0.0 { c.dock_scale.clamp(0.7, 1.5) } else { 1.0 }
}

pub(crate) fn dock_gap(c: &Config) -> f64 {
    c.dock_gap.filter(|g| g.is_finite()).map_or(4.0, |g| g.clamp(0.0, 24.0))
}

pub(crate) fn divider_width(c: &Config) -> f64 {
    c.divider_width.filter(|w| w.is_finite()).map_or(1.0, |w| w.clamp(1.0, 6.0))
}

pub(crate) fn icon_size(c: &Config) -> f64 {
    c.icon_size.filter(|v| v.is_finite()).map_or(30.0, |v| v.clamp(20.0, 44.0))
}

pub(crate) fn icon_gap(c: &Config) -> f64 {
    c.icon_gap.filter(|v| v.is_finite()).map_or(6.0, |v| v.clamp(0.0, 16.0))
}

pub(crate) fn dock_radius(c: &Config) -> f64 {
    c.dock_radius.filter(|v| v.is_finite()).map_or(22.0, |v| v.clamp(0.0, 32.0))
}

pub(crate) fn dock_border(c: &Config) -> f64 {
    c.dock_border.filter(|v| v.is_finite()).map_or(1.0, |v| v.clamp(0.0, 4.0))
}

/// `#rrggbb` (any case) or empty; anything else becomes empty.
pub(crate) fn valid_color(s: &str) -> String {
    let ok = s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit());
    if ok { s.to_lowercase() } else { String::new() }
}

pub(crate) fn current() -> Result<Config, String> {
    Config::try_load(&config::config_path())
}

pub(crate) fn save_and_emit(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let r = cfg.save(&config::config_path());
    r.map_err(|e| e.to_string())?;
    let _ = app.emit("settings://changed", Settings { layout: cfg.layout.clone(), theme: cfg.theme.clone(), bring_to_main: cfg.bring_to_main, bring_key: cfg.bring_key.clone(), floating: cfg.floating.clone(), scales: cfg.scales.clone(), brightness_layout: cfg.brightness_layout.clone(), indicator: cfg.indicator.clone(), start_icon: cfg.start_icon.clone(), dock_monitor: cfg.dock_monitor.clone(), dock_scale: dock_scale(cfg), dock_gap: dock_gap(cfg), dock_autohide: cfg.dock_autohide, hover_lift: cfg.hover_lift.unwrap_or(true), divider_width: divider_width(cfg), icon_size: icon_size(cfg), icon_gap: icon_gap(cfg), dock_radius: dock_radius(cfg), dock_border: dock_border(cfg), dock_text: valid_color(&cfg.dock_text), dock_accent: valid_color(&cfg.dock_accent) });
    Ok(())
}

#[tauri::command]
pub fn get_settings() -> Settings {
    let c = Config::load(&config::config_path());
    let (ds, dg, dw, isz, igp, dr, db) = (dock_scale(&c), dock_gap(&c), divider_width(&c), icon_size(&c), icon_gap(&c), dock_radius(&c), dock_border(&c));
    Settings { layout: c.layout.sanitized(), theme: c.theme.sanitized(), bring_to_main: c.bring_to_main, bring_key: c.bring_key, floating: c.floating, scales: c.scales, brightness_layout: c.brightness_layout, indicator: c.indicator.clone(), dock_monitor: c.dock_monitor.clone(), dock_scale: ds, dock_gap: dg, dock_autohide: c.dock_autohide, hover_lift: c.hover_lift.unwrap_or(true), divider_width: dw, icon_size: isz, icon_gap: igp, dock_radius: dr, dock_border: db, dock_text: valid_color(&c.dock_text), dock_accent: valid_color(&c.dock_accent), start_icon: c.start_icon.clone() }
}

#[tauri::command]
pub fn set_layout(app: AppHandle, layout: LayoutCfg) -> Result<(), String> {
    let mut c = current()?; // a bad file is never overwritten
    c.layout = layout.sanitized();
    save_and_emit(&app, &c)
}

/// Async: sync commands run on the main thread, and a slider fires this many times a second.
#[tauri::command]
pub async fn set_theme(app: AppHandle, theme: ThemeCfg) -> Result<(), String> {
    let mut c = current()?;
    c.theme = theme.sanitized();
    crate::shell::set_clear(&app, c.theme.opacity as f32);
    save_and_emit(&app, &c)
}

#[tauri::command]
pub fn set_widget_scale(app: AppHandle, id: String, scale: f64) -> Result<(), String> {
    let mut c = current()?;
    if !config::WIDGETS.contains(&id.as_str()) {
        return Err("bilinmeyen widget".into());
    }
    let s = if scale.is_finite() { (scale.clamp(0.6, 3.0) * 100.0).round() / 100.0 } else { 1.0 };
    if (s - 1.0).abs() < 0.005 {
        c.scales.remove(&id);
    } else {
        c.scales.insert(id, s);
    }
    save_and_emit(&app, &c)
}

#[tauri::command]
pub fn set_indicator(app: AppHandle, value: String) -> Result<(), String> {
    let mut c = current()?;
    c.indicator = if ["bigdot", "line"].contains(&value.as_str()) { value } else { "dot".into() };
    save_and_emit(&app, &c)
}

/// Screen, size and edge gap of the dock; applied at once.
#[tauri::command]
pub async fn set_dock(app: AppHandle, monitor: String, scale: f64, gap: f64) -> Result<(), String> {
    let mut c = current()?;
    c.dock_monitor = monitor;
    c.dock_scale = if scale.is_finite() { (scale.clamp(0.7, 1.5) * 100.0).round() / 100.0 } else { 1.0 };
    c.dock_gap = Some(if gap.is_finite() { gap.clamp(0.0, 24.0).round() } else { 4.0 });
    save_and_emit(&app, &c)?;
    crate::shell::apply_dock(&app);
    Ok(())
}

/// Async so a slider's stream of calls stays off the main thread.
#[tauri::command]
pub async fn set_divider_width(app: AppHandle, value: f64) -> Result<(), String> {
    let mut c = current()?;
    c.divider_width = Some(if value.is_finite() { (value.clamp(1.0, 6.0) * 2.0).round() / 2.0 } else { 1.0 });
    save_and_emit(&app, &c)
}

/// App icon size and spacing; async so a slider's stream of calls stays off the main thread.
#[tauri::command]
pub async fn set_icons(app: AppHandle, size: f64, gap: f64) -> Result<(), String> {
    let mut c = current()?;
    c.icon_size = Some(if size.is_finite() { size.clamp(20.0, 44.0).round() } else { 30.0 });
    c.icon_gap = Some(if gap.is_finite() { gap.clamp(0.0, 16.0).round() } else { 6.0 });
    save_and_emit(&app, &c)
}

/// Dock corner radius, border width, text color and accent; applied at once.
#[tauri::command]
pub async fn set_look(app: AppHandle, radius: f64, border: f64, text: String, accent: String) -> Result<(), String> {
    let mut c = current()?;
    c.dock_radius = Some(if radius.is_finite() { radius.clamp(0.0, 32.0).round() } else { 22.0 });
    c.dock_border = Some(if border.is_finite() { (border.clamp(0.0, 4.0) * 2.0).round() / 2.0 } else { 1.0 });
    c.dock_text = valid_color(&text);
    c.dock_accent = valid_color(&accent);
    save_and_emit(&app, &c)?;
    crate::shell::apply_dock(&app); // the glass clip follows the new radius
    Ok(())
}

#[tauri::command]
pub fn set_hover_lift(app: AppHandle, on: bool) -> Result<(), String> {
    let mut c = current()?;
    c.hover_lift = Some(on);
    save_and_emit(&app, &c)
}

#[tauri::command]
pub async fn set_autohide(app: AppHandle, on: bool) -> Result<(), String> {
    let mut c = current()?;
    c.dock_autohide = on;
    save_and_emit(&app, &c)?;
    crate::shell::apply_dock(&app);
    Ok(())
}

#[tauri::command]
pub fn autostart_get() -> bool {
    crate::autostart::enabled()
}

#[tauri::command]
pub fn autostart_set(app: AppHandle, on: bool) -> Result<(), String> {
    let mut c = current()?;
    c.autostart = Some(on);
    save_and_emit(&app, &c)?;
    crate::autostart::set(on)
}

#[tauri::command]
pub fn set_start_icon(app: AppHandle, value: String) -> Result<(), String> {
    let mut c = current()?;
    c.start_icon = if ["classic", "outline", "rounded", "aterya", "grid", "dots"].contains(&value.as_str()) { value } else { "windows".into() };
    save_and_emit(&app, &c)
}

#[tauri::command]
pub fn set_brightness_layout(app: AppHandle, value: String) -> Result<(), String> {
    let mut c = current()?;
    c.brightness_layout = if value == "column" { "column".into() } else { "row".into() };
    save_and_emit(&app, &c)
}

#[tauri::command]
pub fn set_bring_to_main(app: AppHandle, value: bool) -> Result<(), String> {
    let mut c = current()?;
    c.bring_to_main = value;
    save_and_emit(&app, &c)
}

#[tauri::command]
pub fn set_bring_key(app: AppHandle, value: String) -> Result<(), String> {
    let mut c = current()?;
    c.bring_key = if config::BRING_KEYS.contains(&value.as_str()) { value } else { "always".into() };
    save_and_emit(&app, &c)
}

/// Async so the window is built off the main thread (building inside a main-thread closure deadlocks).
#[tauri::command]
pub async fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        return w.set_focus().map_err(|e| e.to_string());
    }
    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("index.html#settings".into()))
        .title("Aterya")
        .decorations(false)
        .background_color(tauri::window::Color(11, 11, 13, 255)) // no white flash while the page loads
        .inner_size(1120.0, 800.0)
        .min_inner_size(960.0, 680.0)
        .center()
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The dock's own icon in the notification area (hidden icons): left click opens settings, menu has Exit.
pub fn tray_icon(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("settings", "Ayarlar").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("restart", "Görev çubuğunu yeniden başlat").build(app)?)
        .item(&MenuItemBuilder::with_id("quit", "Çıkış").build(app)?)
        .build()?;
    let mut b = TrayIconBuilder::with_id("dock")
        .tooltip("Aterya çalışıyor")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "settings" => open_in_background(app),
            "restart" => crate::shell::restart_dock(app),
            "quit" => app.exit(0), // teardown in lib.rs gives the Windows taskbar back
            _ => {}
        })
        .on_tray_icon_event(|t, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                open_in_background(t.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.build(app).map(|_| ())
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    crate::shell::restart_dock(&app);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn open_in_background(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = open_settings(app).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_roundtrip_through_sanitize() {
        let s = Settings { layout: LayoutCfg::default().sanitized(), theme: ThemeCfg::default().sanitized(), bring_to_main: false, bring_key: String::new(), floating: vec![], scales: Default::default(), brightness_layout: String::new(), indicator: String::new(), start_icon: String::new(), dock_monitor: String::new(), dock_scale: 1.0, dock_gap: 4.0, dock_autohide: false, hover_lift: true, divider_width: 1.0, icon_size: 30.0, icon_gap: 6.0, dock_radius: 22.0, dock_border: 1.0, dock_text: String::new(), dock_accent: String::new() };
        assert_eq!(s.layout, LayoutCfg::default());
        assert_eq!(s.theme, ThemeCfg::default());
    }
}
