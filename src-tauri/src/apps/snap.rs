//! Last-seen pictures of windows. A minimized window cannot be shown live (DWM draws nothing for it), so a
//! small picture is taken while the window is still on screen: when it starts to minimize, when it loses the
//! foreground, and whenever the preview shows it. The preview draws that picture for a minimized window.
use base64::Engine;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
        BitBlt, CAPTUREBLT, GetDC, HALFTONE, HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt,
    },
    Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
    UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible},
};

/// Longest side of a stored picture, physical px: sharp enough for the preview's 200x120 slot on a 200% screen.
const MAX_SIDE: i32 = 480;
const KEEP: usize = 60;
/// Foreground changes retake a picture at most this often per window.
const REFRESH: Duration = Duration::from_secs(3);

struct Snap {
    /// `data:image/png;base64,...`
    url: String,
    taken: Instant,
}

static CACHE: Mutex<Option<HashMap<isize, Snap>>> = Mutex::new(None);

fn cache<R>(f: impl FnOnce(&mut HashMap<isize, Snap>) -> R) -> R {
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

/// Keeps the picture of the window in front fresh (every 2 s), so a window that is minimized has a recent one:
/// by the time a minimize event reaches the app the window is already gone from the screen.
pub fn start() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(2));
        let root = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) };
        if root.0.is_null() {
            continue;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(root, Some(&mut pid)) };
        if pid == std::process::id() {
            continue; // the dock and its popups
        }
        let hwnd = root.0 as isize;
        if let Ok(url) = capture(hwnd) {
            cache(|c| {
                c.insert(hwnd, Snap { url, taken: Instant::now() });
            });
        }
    });
}

/// The last picture of `hwnd`, if one was taken.
pub fn get(hwnd: isize) -> Option<String> {
    cache(|c| c.get(&hwnd).map(|s| s.url.clone()))
}

/// Picture of the window as it is now, stored under its handle. `force` ignores the refresh interval.
/// Runs on a thread of its own: taking a picture can take tens of milliseconds.
pub fn capture_soon(hwnd: isize, force: bool) {
    if !force && cache(|c| c.get(&hwnd).is_some_and(|s| s.taken.elapsed() < REFRESH)) {
        return;
    }
    std::thread::spawn(move || {
        let got = capture(hwnd);
        log::info!("[snap] window {hwnd}: {}", match &got { Ok(u) => format!("{} bytes", u.len()), Err(why) => format!("none ({why})") });
        if let Ok(url) = got {
            cache(|c| {
                c.insert(hwnd, Snap { url, taken: Instant::now() });
                if c.len() > KEEP {
                    c.retain(|&h, _| unsafe { IsWindow(Some(HWND(h as *mut _))) }.as_bool());
                }
                while c.len() > KEEP {
                    let Some((&old, _)) = c.iter().min_by_key(|(_, s)| s.taken) else { break };
                    c.remove(&old);
                }
            });
        }
    });
}

/// Largest size with the source's aspect ratio whose long side is at most `MAX_SIDE`.
fn target_size(w: i32, h: i32) -> (i32, i32) {
    let s = (MAX_SIDE as f64 / w.max(h) as f64).min(1.0);
    (((w as f64 * s).round() as i32).max(1), ((h as f64 * s).round() as i32).max(1))
}

/// Nearly all pixels the same dark: the window did not paint into the picture (some games and protected content).
fn blank(rgba: &[u8]) -> bool {
    let dark = rgba.chunks_exact(4).filter(|p| p[0] < 6 && p[1] < 6 && p[2] < 6).count();
    dark * 100 >= (rgba.len() / 4) * 99
}

fn capture(hwnd: isize) -> Result<String, &'static str> {
    let h = HWND(hwnd as *mut _);
    unsafe {
        if !IsWindow(Some(h)).as_bool() {
            return Err("gone");
        }
        if !IsWindowVisible(h).as_bool() {
            return Err("not visible");
        }
        if IsIconic(h).as_bool() {
            return Err("already minimized");
        }
        let mut r = RECT::default();
        GetWindowRect(h, &mut r).map_err(|_| "no rect")?;
        let (w, ht) = (r.right - r.left, r.bottom - r.top);
        if w < 40 || ht < 40 {
            return Err("too small");
        }
        let (tw, th) = target_size(w, ht);
        let screen = GetDC(None);
        let src_dc = CreateCompatibleDC(Some(screen));
        let src_bmp = CreateCompatibleBitmap(screen, w, ht);
        let old_src = SelectObject(src_dc, HGDIOBJ(src_bmp.0));
        // The window in front is copied from the screen: right for every kind of window, GPU-drawn ones included.
        // Any other may be covered, so it is asked to paint itself (PW_RENDERFULLCONTENT = 2, which also covers
        // windows drawn by DirectComposition); some of those answer with a black picture, which is dropped.
        let drawn = if GetAncestor(GetForegroundWindow(), GA_ROOT) == h {
            BitBlt(src_dc, 0, 0, w, ht, Some(screen), r.left, r.top, SRCCOPY | CAPTUREBLT).is_ok()
        } else {
            PrintWindow(h, src_dc, PRINT_WINDOW_FLAGS(2)).as_bool()
        };

        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: tw, biHeight: -th, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(screen), &bi, DIB_RGB_COLORS, &mut bits, None, 0);
        let mut out = Err(if drawn { "no bitmap" } else { "PrintWindow failed" });
        if drawn && let Ok(dib) = dib && !bits.is_null() {
            let dst_dc = CreateCompatibleDC(Some(screen));
            let old_dst = SelectObject(dst_dc, HGDIOBJ(dib.0));
            SetStretchBltMode(dst_dc, HALFTONE);
            let _ = SetBrushOrgEx(dst_dc, 0, 0, None);
            if StretchBlt(dst_dc, 0, 0, tw, th, Some(src_dc), 0, 0, w, ht, SRCCOPY).as_bool() {
                let bgra = std::slice::from_raw_parts(bits as *const u8, (tw * th * 4) as usize);
                let mut rgba = Vec::with_capacity(bgra.len());
                for p in bgra.chunks_exact(4) {
                    rgba.extend_from_slice(&[p[2], p[1], p[0], 255]);
                }
                out = if blank(&rgba) { Err("picture is blank") } else { encode(&rgba, tw as u32, th as u32).ok_or("png failed") };
            }
            SelectObject(dst_dc, old_dst);
            let _ = DeleteDC(dst_dc);
        }
        if let Ok(dib) = dib {
            let _ = DeleteObject(HGDIOBJ(dib.0));
        }
        SelectObject(src_dc, old_src);
        let _ = DeleteObject(HGDIOBJ(src_bmp.0));
        let _ = DeleteDC(src_dc);
        ReleaseDC(None, screen);
        out
    }
}

fn encode(rgba: &[u8], w: u32, h: u32) -> Option<String> {
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        enc.write_header().ok()?.write_image_data(rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png_bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_keeps_aspect_and_never_grows() {
        assert_eq!(target_size(1920, 1080), (480, 270));
        assert_eq!(target_size(1080, 1920), (270, 480));
        assert_eq!(target_size(300, 200), (300, 200));
    }

    #[test]
    fn blank_detects_a_black_picture() {
        assert!(blank(&[0, 0, 0, 255].repeat(100)));
        let mut mostly = [0, 0, 0, 255].repeat(100);
        mostly[0] = 200;
        assert!(blank(&mostly), "one lit pixel in a hundred is still blank");
        let mut half = [0, 0, 0, 255].repeat(100);
        for p in half.chunks_exact_mut(4).take(50) {
            p[0] = 200;
        }
        assert!(!blank(&half));
    }
}
