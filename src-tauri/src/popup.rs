//! A small popup window next to the dock (calendar, ...): built hidden, shown at an icon on a click, closed when
//! it loses focus. One `Popup` per window; the page tells its content height so the window fits it.
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

const ABOVE_DOCK_CSS: f64 = 8.0;

pub struct Popup {
    /// Window label and the page hash (`index.html#<label>`).
    pub label: &'static str,
    width_css: i32,
    /// Event telling the page it opened (true) or closed (false).
    open_event: &'static str,
    open: AtomicBool,
    /// Content height (CSS px) the page last asked for; the popup opens at that size.
    last_h: AtomicI32,
    /// The widget's position along the dock, physical px.
    icon: AtomicI32,
    /// A blur closes the popup before the click that caused it arrives: a close just now makes that click the closing one.
    closed_at: Mutex<Option<Instant>>,
}

#[derive(Serialize, Clone)]
struct Open(bool);

impl Popup {
    pub const fn new(label: &'static str, width_css: i32, open_event: &'static str) -> Self {
        Self { label, width_css, open_event, open: AtomicBool::new(false), last_h: AtomicI32::new(0), icon: AtomicI32::new(0), closed_at: Mutex::new(None) }
    }

    pub fn ensure(&'static self, app: &AppHandle) -> Result<WebviewWindow, String> {
        if let Some(w) = app.get_webview_window(self.label) {
            return Ok(w);
        }
        let w = WebviewWindowBuilder::new(app, self.label, WebviewUrl::App(format!("index.html#{}", self.label).into()))
            .title("")
            .additional_browser_args(crate::BROWSER_ARGS)
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .shadow(false)
            .visible(false)
            .inner_size(self.width_css as f64, 300.0)
            .build()
            .map_err(|e| e.to_string())?;
        let h = w.hwnd().map_err(|e| e.to_string())?.0 as isize;
        w.run_on_main_thread(move || crate::nocaption::apply(h)).map_err(|e| e.to_string())?;
        let app2 = app.clone();
        w.on_window_event(move |e| {
            if matches!(e, WindowEvent::Focused(false)) {
                self.hide(&app2);
            }
        });
        Ok(w)
    }

    /// Builds the (hidden) window ahead of its first use.
    pub fn warm(&'static self, app: &AppHandle) {
        let _ = self.ensure(app);
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    pub fn hide(&'static self, app: &AppHandle) {
        if self.open.swap(false, Ordering::SeqCst) {
            *self.closed_at.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        }
        let _ = app.emit(self.open_event, Open(false).0);
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(190)); // the page fades out first
            if self.open.load(Ordering::SeqCst) {
                return;
            }
            let a = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(Ok(h)) = a.get_webview_window(self.label).map(|w| w.hwnd()) {
                    if !self.open.load(Ordering::SeqCst) {
                        unsafe {
                            let _ = ShowWindow(h, SW_HIDE);
                        }
                    }
                }
            });
        });
    }

    fn place(&'static self, app: &AppHandle, height_css: f64, activate: bool) -> Result<(), String> {
        let win = self.ensure(app)?;
        let me = win.hwnd().map_err(|e| e.to_string())?;
        let dock = crate::shell::dock_hwnd();
        let k = unsafe { GetDpiForWindow(dock) }.max(96) as f64 / 96.0;
        let px = |v: f64| (v * k).round() as i32;
        let (w, h) = (px(self.width_css as f64), px(height_css.clamp(120.0, 1100.0)));
        let mut dr = RECT::default();
        unsafe { GetWindowRect(dock, &mut dr) }.map_err(|e| e.to_string())?;
        let mon = crate::shell::primary_monitor();
        let (x, y) = crate::shell::popup_pos(dr, self.icon.load(Ordering::SeqCst), (w, h), px(ABOVE_DOCK_CSS), px(8.0), mon);
        unsafe {
            let flags = if activate { SWP_SHOWWINDOW } else { SWP_NOACTIVATE };
            let _ = SetWindowPos(me, Some(HWND_TOPMOST), x, y, w, h, flags);
            if activate {
                let _ = SetForegroundWindow(me);
            }
        }
        Ok(())
    }

    /// The widget was clicked. `icon_cx`: its center along the dock in the dock's CSS px. Opens, or closes when open.
    pub async fn toggle(&'static self, app: AppHandle, icon_cx: f64, default_h: f64) -> Result<(), String> {
        let just_closed = self.closed_at.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|t| t.elapsed() < Duration::from_millis(400));
        if just_closed || self.is_open() {
            *self.closed_at.lock().unwrap_or_else(|e| e.into_inner()) = None;
            let a = app.clone();
            return app.run_on_main_thread(move || self.hide(&a)).map_err(|e| e.to_string());
        }
        let mut dr = RECT::default();
        unsafe { GetWindowRect(crate::shell::dock_hwnd(), &mut dr) }.map_err(|e| e.to_string())?;
        self.icon.store(crate::shell::along_pos(dr, icon_cx), Ordering::SeqCst);
        self.ensure(&app)?; // build off the main thread: creating a window from inside a main-thread closure deadlocks
        self.open.store(true, Ordering::SeqCst);
        let height = match self.last_h.load(Ordering::SeqCst) {
            0 => default_h,
            h => h as f64,
        };
        let a = app.clone();
        app.run_on_main_thread(move || {
            if self.place(&a, height, true).is_err() {
                self.open.store(false, Ordering::SeqCst);
            } else {
                let _ = a.emit(self.open_event, true);
            }
        })
        .map_err(|e| e.to_string())
    }

    /// The page's content changed height.
    pub async fn fit(&'static self, app: AppHandle, height: f64) -> Result<(), String> {
        self.last_h.store(height.round() as i32, Ordering::SeqCst);
        if !self.is_open() {
            return Ok(());
        }
        let a = app.clone();
        app.run_on_main_thread(move || {
            let _ = self.place(&a, height, false);
        })
        .map_err(|e| e.to_string())
    }
}
