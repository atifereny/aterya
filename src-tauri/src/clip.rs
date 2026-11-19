//! Clipboard history: the last texts and pictures (screenshots) copied, newest first, kept in memory only (never
//! written to disk). Items a password manager marks as excluded are skipped.
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use windows::{
    Win32::{
        Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            DataExchange::{AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData},
            LibraryLoader::GetModuleHandleW,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG, RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
            WNDCLASSW,
        },
    },
    core::w,
};

const MAX_ITEMS: usize = 25;
const MAX_LEN: usize = 5000; // chars kept per text
const MAX_IMAGE_BYTES: usize = 48 << 20; // one picture
const MAX_IMAGES_TOTAL: usize = 128 << 20; // all pictures together
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const WM_CLIPBOARDUPDATE: u32 = 0x031D;
const THUMB: (u32, u32) = (96, 96);

#[derive(Clone)]
enum Entry {
    Text(String),
    Image { dib: Vec<u8>, w: u32, h: u32, thumb: String },
}

/// What the popup list needs: a label and, for pictures, a small PNG data URL.
pub struct Shown {
    pub label: String,
    pub thumb: Option<String>,
}

static HISTORY: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

fn hist() -> std::sync::MutexGuard<'static, Vec<Entry>> {
    HISTORY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Newest first, at most `n` items.
pub fn history(n: usize) -> Vec<Shown> {
    hist()
        .iter()
        .take(n)
        .map(|e| match e {
            Entry::Text(t) => Shown { label: t.clone(), thumb: None },
            Entry::Image { w, h, thumb, .. } => Shown { label: format!("Ekran görüntüsü · {w}×{h}"), thumb: Some(thumb.clone()) },
        })
        .collect()
}

pub fn count() -> usize {
    hist().len()
}

fn changed() {
    if let Some(app) = APP.get() {
        let _ = app.emit("clip://changed", count());
    }
}

pub fn clear() {
    hist().clear();
    changed();
}

/// Put history item `i` (newest = 0) back on the clipboard; it becomes the newest entry again.
pub fn copy(i: usize) {
    let Some(e) = hist().get(i).cloned() else { return };
    match e {
        Entry::Text(t) => set_data(CF_UNICODETEXT, &t.encode_utf16().chain(Some(0)).flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>()),
        Entry::Image { dib, .. } => set_data(CF_DIB, &dib),
    }
}

fn set_data(format: u32, bytes: &[u8]) {
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) {
            let p = GlobalLock(h) as *mut u8;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                let _ = GlobalUnlock(h);
                let _ = SetClipboardData(format, Some(HANDLE(h.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

/// A packed DIB (header + pixels) as a small PNG data URL and its pixel size. 24 and 32 bit only.
fn dib_thumb(dib: &[u8]) -> Option<(u32, u32, String)> {
    let u32_at = |o: usize| dib.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let size = u32_at(0)? as usize;
    let (w, h_raw) = (u32_at(4)? as i32, u32_at(8)? as i32);
    let bits = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]) as usize;
    let comp = u32_at(16)?;
    let clr_used = u32_at(32).unwrap_or(0) as usize;
    if w <= 0 || h_raw == 0 || !(bits == 24 || bits == 32) || !(comp == 0 || comp == 3) {
        return None;
    }
    let (w, h, flip) = (w as usize, h_raw.unsigned_abs() as usize, h_raw > 0);
    let masks = if comp == 3 && size == 40 { 12 } else { 0 };
    let off = size + masks + clr_used * 4;
    let stride = (w * bits).div_ceil(32) * 4;
    if dib.len() < off + stride * h {
        return None;
    }
    let scale = (THUMB.0 as f64 / w as f64).min(THUMB.1 as f64 / h as f64).min(1.0);
    let (tw, th) = (((w as f64 * scale).round() as usize).max(1), ((h as f64 * scale).round() as usize).max(1));
    let mut px = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        let sy = ty * h / th;
        let row = if flip { h - 1 - sy } else { sy };
        for tx in 0..tw {
            let sx = tx * w / tw;
            let p = off + row * stride + sx * bits / 8;
            px.extend_from_slice(&[dib[p + 2], dib[p + 1], dib[p], 255]);
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, tw as u32, th as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().ok()?.write_image_data(&px).ok()?;
    }
    Some((w as u32, h as u32, format!("data:image/png;base64,{}", STANDARD.encode(out))))
}

fn same_image(a: &Entry, dib2: &[u8]) -> bool {
    matches!(a, Entry::Image { dib, .. } if dib.len() == dib2.len() && dib[..dib.len().min(4096)] == dib2[..dib2.len().min(4096)] && dib[dib.len() / 2..dib.len() / 2 + 64.min(dib.len() / 2)] == dib2[dib2.len() / 2..dib2.len() / 2 + 64.min(dib2.len() / 2)])
}

/// Text or a picture from the clipboard, unless the source asked not to be recorded.
fn read_entry(hwnd: HWND) -> Option<Entry> {
    unsafe {
        let skip = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(Some(hwnd)).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !opened {
            return None;
        }
        let out = (|| {
            if skip != 0 && IsClipboardFormatAvailable(skip).is_ok() {
                return None;
            }
            if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
                let g = HGLOBAL(h.0);
                let p = GlobalLock(g) as *const u16;
                if !p.is_null() {
                    let mut n = 0;
                    while *p.add(n) != 0 && n < MAX_LEN * 2 {
                        n += 1;
                    }
                    let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
                    let _ = GlobalUnlock(g);
                    let t: String = s.chars().take(MAX_LEN).collect();
                    if !t.trim().is_empty() {
                        return Some(Entry::Text(t));
                    }
                }
            }
            let h = GetClipboardData(CF_DIB).ok()?;
            let g = HGLOBAL(h.0);
            let size = GlobalSize(g);
            if size == 0 || size > MAX_IMAGE_BYTES {
                return None;
            }
            let p = GlobalLock(g) as *const u8;
            if p.is_null() {
                return None;
            }
            let dib = std::slice::from_raw_parts(p, size).to_vec();
            let _ = GlobalUnlock(g);
            let (w, h, thumb) = dib_thumb(&dib)?;
            Some(Entry::Image { dib, w, h, thumb })
        })();
        let _ = CloseClipboard();
        out
    }
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        if let Some(e) = read_entry(h) {
            let mut v = hist();
            match &e {
                Entry::Text(t) => v.retain(|x| !matches!(x, Entry::Text(o) if o == t)),
                Entry::Image { dib, .. } => v.retain(|x| !same_image(x, dib)),
            }
            v.insert(0, e);
            v.truncate(MAX_ITEMS);
            // Keep the pictures' memory bounded: drop the oldest ones past the total cap.
            let mut total = 0usize;
            v.retain(|x| match x {
                Entry::Image { dib, .. } => {
                    total += dib.len();
                    total <= MAX_IMAGES_TOTAL
                }
                _ => true,
            });
            drop(v);
            changed();
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(h, msg, wp, lp) }
}

/// Starts listening for clipboard changes on its own thread.
pub fn start(app: AppHandle) {
    let _ = APP.set(app);
    std::thread::spawn(|| unsafe {
        let Ok(hinst) = GetModuleHandleW(None) else { return };
        let class = w!("AteryaClipboard");
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hinst.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        let Ok(h) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hinst.into()), None) else { return };
        if AddClipboardFormatListener(h).is_err() {
            log::warn!("[clip] no clipboard listener");
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

#[tauri::command]
pub fn clip_count() -> usize {
    count()
}

#[tauri::command]
pub fn clip_clear() {
    clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dib(w: i32, h: i32, bits: u16) -> Vec<u8> {
        let stride = (w as usize * bits as usize).div_ceil(32) * 4;
        let mut v = Vec::new();
        v.extend_from_slice(&40u32.to_le_bytes());
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());
        v.extend_from_slice(&[0u8; 24]);
        v.extend(std::iter::repeat_n(200u8, stride * h.unsigned_abs() as usize));
        v
    }

    #[test]
    fn thumbnail_of_32_and_24_bit() {
        for bits in [24, 32] {
            let (w, h, url) = dib_thumb(&dib(400, 200, bits)).unwrap();
            assert_eq!((w, h), (400, 200));
            assert!(url.starts_with("data:image/png;base64,"));
        }
    }

    #[test]
    fn rejects_truncated_and_odd_formats() {
        assert!(dib_thumb(&dib(10, 10, 32)[..60]).is_none());
        assert!(dib_thumb(&dib(10, 10, 8)).is_none());
        assert!(dib_thumb(&[]).is_none());
    }
}
