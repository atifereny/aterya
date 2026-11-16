//! Blurred backdrop behind the pill: a DirectComposition visual painted with the host backdrop
//! brush (the same DWM blur Windows acrylic uses) and clipped by an anti-aliased rounded-rect
//! geometry. A slightly larger window region keeps clicks outside the pill going to windows below.
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::{w, Interface, PCWSTR};
use windows::System::DispatcherQueueController;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{CompositionRoundedRectangleGeometry, Compositor, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_HOSTBACKDROPBRUSH, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, CreateRoundRectRgn, SetWindowRgn};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::System::WinRT::{CreateDispatcherQueueController, DispatcherQueueOptions, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE, MA_NOACTIVATE, STYLESTRUCT,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WINDOW_STYLE, WM_MOUSEACTIVATE,
    WM_NCACTIVATE, WM_SETTINGCHANGE, WM_STYLECHANGING, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    WS_SYSMENU,
};
use windows_core::BOOL;
use windows_numerics::{Vector2, Vector3};

const CAPTION_BITS: WINDOW_STYLE = WINDOW_STYLE(WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0);

unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, on_blur: usize) -> LRESULT {
    match msg {
        WM_STYLECHANGING if wp.0 as i32 == GWL_STYLE.0 => {
            let ss = unsafe { &mut *(lp.0 as *mut STYLESTRUCT) };
            ss.styleNew &= !CAPTION_BITS.0;
        }
        // A taskbar must never take focus from the app the user is in (WS_EX_NOACTIVATE comes from
        // `focusable: false` in tauri.conf.json; this covers clicks routed through the webview).
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        // lParam -1: let activation change without DefWindowProc painting the inactive frame over the pill.
        WM_NCACTIVATE => return unsafe { DefSubclassProc(hwnd, msg, wp, LPARAM(-1)) },
        WM_SETTINGCHANGE if lp.0 != 0 && unsafe { PCWSTR(lp.0 as _).to_string() }.is_ok_and(|s| s == "ImmersiveColorSet") => {
            let blur = set_blur(hwnd);
            let on_blur: fn(bool) = unsafe { std::mem::transmute(on_blur) };
            on_blur(blur);
        }
        _ => {
            let r = unsafe { DefSubclassProc(hwnd, msg, wp, lp) };
            super::on_message(hwnd, msg, wp, lp); // AppBar, DPI, display, session, TaskbarCreated
            return r;
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

static BLUR: AtomicBool = AtomicBool::new(false);

/// Whether DWM blur is live behind the pill (false: frontend should paint an opaque pill).
pub fn blur_active() -> bool {
    BLUR.load(Ordering::Relaxed)
}

fn transparency_enabled() -> bool {
    let (mut v, mut n) = (1u32, 4u32);
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("EnableTransparency"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut _ as _), Some(&mut n))
    };
    r.is_err() || v != 0 // missing value means the default, which is on
}

/// Composition objects; they live on the window's (main) thread.
struct Backdrop {
    _queue: DispatcherQueueController,
    _target: DesktopWindowTarget,
    visual: SpriteVisual,
    shape: CompositionRoundedRectangleGeometry,
}

thread_local! {
    static BACKDROP: RefCell<Option<Backdrop>> = const { RefCell::new(None) };
}

fn create_backdrop(hwnd: HWND) -> windows::core::Result<Backdrop> {
    let opts = DispatcherQueueOptions {
        dwSize: size_of::<DispatcherQueueOptions>() as u32,
        threadType: DQTYPE_THREAD_CURRENT,
        apartmentType: DQTAT_COM_NONE,
    };
    let queue = unsafe { CreateDispatcherQueueController(opts)? }; // Compositor needs one on this thread
    let compositor = Compositor::new()?;
    // Not topmost: the visual sits under the WebView2 child, so the CSS pill paints over the blur.
    let target = unsafe { compositor.cast::<ICompositorDesktopInterop>()?.CreateDesktopWindowTarget(hwnd, false)? };
    let visual = compositor.CreateSpriteVisual()?;
    visual.SetBrush(&compositor.CreateHostBackdropBrush()?)?;
    let shape = compositor.CreateRoundedRectangleGeometry()?;
    visual.SetClip(&compositor.CreateGeometricClipWithGeometry(&shape)?)?; // anti-aliased edge
    target.SetRoot(&visual)?;
    Ok(Backdrop { _queue: queue, _target: target, visual, shape })
}

/// Blur on when "Transparency effects" is on and the backdrop exists; otherwise the frontend
/// paints a solid pill. Must run on the window's thread.
fn set_blur(_hwnd: HWND) -> bool {
    let want = transparency_enabled();
    let blur = BACKDROP.with_borrow(|b| b.as_ref().is_some_and(|b| b.visual.SetIsVisible(want).is_ok())) && want;
    log::info!("[glass] transparency effects {}, blur {}", if want { "on" } else { "off" }, blur);
    BLUR.store(blur, Ordering::Relaxed);
    blur
}

/// Blur (re-checked live on theme changes, reported through `on_blur`), dark mode,
/// and an empty region until the pill is measured. Must run on the window's thread.
pub fn apply(hwnd: HWND, on_blur: fn(bool)) {
    unsafe {
        // tao keeps WS_CAPTION on undecorated windows (and re-applies it on focus changes); with the
        // WM_NCACTIVATE override this stops an inactive frame being painted as a light band on the pill.
        let _ = SetWindowSubclass(hwnd, Some(subclass), 1, on_blur as usize);
        SetWindowLongPtrW(hwnd, GWL_STYLE, GetWindowLongPtrW(hwnd, GWL_STYLE) & !(CAPTION_BITS.0 as isize));
        let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, flags);
        let on = BOOL(1);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &on as *const _ as _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH, &on as *const _ as _, 4);
        match create_backdrop(hwnd) {
            Ok(b) => BACKDROP.set(Some(b)),
            Err(e) => log::warn!("[glass] no composition backdrop ({e}); solid pill"),
        }
        set_blur(hwnd);
        // Nothing visible (or clickable) until the frontend reports the pill bounds.
        SetWindowRgn(hwnd, Some(CreateRectRgn(0, 0, 0, 0)), true);
    }
}

/// CSS px -> physical px at `dpi`.
fn phys(css: f64, dpi: u32) -> f64 {
    css * dpi as f64 / 96.0
}

/// Hit region margin (physical px) around the pill. GDI regions are aliased; keeping the region's
/// stair-step this far outside the anti-aliased clip makes it invisible (only transparent pixels).
const HIT_SLACK: i32 = 2;

/// Place the blur behind the pill (a rounded rect at the top of the window) and make the same
/// area, plus HIT_SLACK, the window's hit region. All arguments are CSS px. Window's thread only.
pub fn set_region(hwnd: HWND, x: f64, width: f64, height: f64, radius: f64) {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let (l, w, h, r) = (phys(x, dpi), phys(width, dpi), phys(height, dpi), phys(radius, dpi));
    BACKDROP.with_borrow(|b| {
        if let Some(b) = b {
            let size = Vector2 { X: w as f32, Y: h as f32 };
            let _ = b.visual.SetOffset(Vector3 { X: l as f32, Y: 0.0, Z: 0.0 });
            let _ = b.visual.SetSize(size);
            let _ = b.shape.SetSize(size);
            let _ = b.shape.SetCornerRadius(Vector2 { X: r as f32, Y: r as f32 });
        }
    });
    let s = HIT_SLACK;
    let (l, rgt, bot, d) = (l.floor() as i32 - s, (l + w).ceil() as i32 + s, h.ceil() as i32 + s, (2.0 * r).round() as i32 + 2 * s);
    unsafe {
        SetWindowRgn(hwnd, Some(CreateRoundRectRgn(l, -s, rgt + 1, bot + 1, d, d)), true); // system owns the region
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_to_physical() {
        assert_eq!(super::phys(64.0, 96), 64.0);
        assert_eq!(super::phys(64.0, 144), 96.0);
        assert_eq!(super::phys(22.0, 120), 27.5);
    }
}
