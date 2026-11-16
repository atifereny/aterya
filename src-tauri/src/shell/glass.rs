//! DWM blur behind the dock window, clipped to the pill by a rounded window region.
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::w;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::Foundation::HWND;
use windows_core::BOOL;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, CreateRoundRectRgn, SetWindowRgn};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE, MA_NOACTIVATE, STYLESTRUCT,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WINDOW_STYLE, WM_MOUSEACTIVATE,
    WM_NCACTIVATE, WM_SETTINGCHANGE, WM_STYLECHANGING, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    WS_SYSMENU,
};
use windows::core::PCWSTR;

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    gradient_color: u32, // AABBGGRR tint under the CSS fill
    animation_id: u32,
}
#[repr(C)]
struct CompositionAttribData {
    attrib: u32,
    data: *mut c_void,
    size: usize,
}
const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

// Undocumented but stable since Win10 1803; used because the documented DWMSBT backdrops
// fall back to a flat colour whenever the window is inactive, and the dock almost always is.
#[link(name = "user32", kind = "raw-dylib")]
unsafe extern "system" {
    fn SetWindowCompositionAttribute(hwnd: HWND, data: *mut CompositionAttribData) -> BOOL;
}

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
        _ => {}
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

/// Acrylic on when "Transparency effects" is on, accent off otherwise: with the setting off DWM
/// paints acrylic as an opaque black slab over the whole window, ignoring the region.
fn set_blur(hwnd: HWND) -> bool {
    let want = transparency_enabled();
    let state = if want { ACCENT_ENABLE_ACRYLICBLURBEHIND } else { ACCENT_DISABLED };
    let mut policy = AccentPolicy { state, flags: 0, gradient_color: 0x10_10_10_10, animation_id: 0 };
    let mut data = CompositionAttribData { attrib: WCA_ACCENT_POLICY, data: &mut policy as *mut _ as _, size: size_of::<AccentPolicy>() };
    let blur = unsafe { SetWindowCompositionAttribute(hwnd, &mut data) }.as_bool() && want;
    eprintln!("[glass] transparency effects {}, blur {}", if want { "on" } else { "off" }, blur);
    BLUR.store(blur, Ordering::Relaxed);
    blur
}

/// Blur (re-checked live on theme changes, reported through `on_blur`), dark mode,
/// and an empty region until the pill is measured.
pub fn apply(hwnd: HWND, on_blur: fn(bool)) {
    unsafe {
        // tao keeps WS_CAPTION on undecorated windows (and re-applies it on focus changes); with the
        // WM_NCACTIVATE override this stops an inactive frame being painted as a light band on the pill.
        let _ = SetWindowSubclass(hwnd, Some(subclass), 1, on_blur as usize);
        SetWindowLongPtrW(hwnd, GWL_STYLE, GetWindowLongPtrW(hwnd, GWL_STYLE) & !(CAPTION_BITS.0 as isize));
        let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, flags);
        let dark = BOOL(1);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        set_blur(hwnd);
        // Nothing visible (or clickable) until the frontend reports the pill bounds.
        SetWindowRgn(hwnd, Some(CreateRectRgn(0, 0, 0, 0)), true);
    }
}

/// CSS px → physical px at the window's current DPI.
fn phys(css: f64, dpi: u32) -> i32 {
    (css * dpi as f64 / 96.0).round() as i32
}

/// Clip the window (paint, blur and hit-testing) to a rounded rect at the top of the window.
/// All arguments are CSS px.
pub fn set_region(hwnd: HWND, x: f64, width: f64, height: f64, radius: f64) {
    unsafe {
        let dpi = GetDpiForWindow(hwnd).max(96);
        let (l, r, b, d) = (phys(x, dpi), phys(x + width, dpi), phys(height, dpi), phys(radius * 2.0, dpi));
        // shortcut: GDI regions are aliased, so the 22px corners have a 1px stair-step; upgrade to a
        // DirectComposition visual if that ever shows. Region also excludes the CSS drop shadow.
        let rgn = CreateRoundRectRgn(l, 0, r + 1, b + 1, d, d);
        SetWindowRgn(hwnd, Some(rgn), true); // the system owns rgn from here
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_to_physical() {
        assert_eq!(super::phys(64.0, 96), 64);
        assert_eq!(super::phys(64.0, 144), 96);
        assert_eq!(super::phys(22.5, 120), 28);
    }
}
