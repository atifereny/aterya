use base64::{Engine, engine::general_purpose::STANDARD};
use std::{collections::HashMap, sync::Mutex};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, SIZE, WPARAM},
        Graphics::Gdi::*,
        UI::WindowsAndMessaging::*,
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY},
    },
    core::HSTRING,
};

static CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
const SIZE_PX: i32 = 32;

/// PNG data URL for a group, cached by group id. Empty string when no icon could be had.
/// `id` is an exe path or `aumid:<AUMID>`. Needs COM initialised on the calling thread.
/// Exe groups prefer the window's own icon; AUMID groups and fallbacks use the shell image factory.
pub fn icon_for(id: &str, hwnd: Option<isize>) -> String {
    if let Some(hit) = CACHE.lock().unwrap().get_or_insert_with(Default::default).get(id) {
        return hit.clone();
    }
    // File Explorer windows carry a generic window icon; its exe/shell icon is the real folder icon.
    let explorer = id.to_ascii_lowercase().contains("explorer");
    let win = hwnd.filter(|_| !id.starts_with("aumid:") && !explorer).and_then(window_icon);
    let url = win.or_else(|| shell_icon(id)).or_else(|| hwnd.and_then(exe_icon)).unwrap_or_default();
    if !url.is_empty() {
        CACHE.lock().unwrap().get_or_insert_with(Default::default).insert(id.into(), url.clone());
    }
    url
}

/// Last resort: the window's process exe. Covers apps whose explicit AppUserModelID is not a real
/// shell item (e.g. League of Legends sets its exe path as AUMID) and that set no window icon.
fn exe_icon(hwnd: isize) -> Option<String> {
    use windows::Win32::System::Threading::*;
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize])).and_then(|path| shell_icon(&path))
    }
}

fn shell_icon(id: &str) -> Option<String> {
    let path = match id.strip_prefix("aumid:") {
        Some(aumid) => format!(r"shell:AppsFolder\{aumid}"),
        None => id.to_string(),
    };
    unsafe {
        let f: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let bmp = f.GetImage(SIZE { cx: SIZE_PX, cy: SIZE_PX }, SIIGBF_ICONONLY).ok()?;
        let url = bitmap_to_png_url(bmp);
        let _ = DeleteObject(bmp.into());
        url
    }
}

fn window_icon(hwnd: isize) -> Option<String> {
    let h = HWND(hwnd as *mut _);
    unsafe {
        let mut icon = 0usize;
        for kind in [ICON_BIG, ICON_SMALL2] {
            let mut r = 0usize;
            SendMessageTimeoutW(h, WM_GETICON, WPARAM(kind as usize), LPARAM(0), SMTO_ABORTIFHUNG, 50, Some(&mut r));
            if r != 0 {
                icon = r;
                break;
            }
        }
        if icon == 0 {
            icon = GetClassLongPtrW(h, GCLP_HICON);
        }
        if icon == 0 {
            return None;
        }
        // Window/class icons are shared with the owner: never DestroyIcon.
        let hicon = HICON(icon as *mut _);
        let dc = CreateCompatibleDC(None);
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SIZE_PX,
            biHeight: -SIZE_PX,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0);
        let out = dib.ok().and_then(|dib| {
            let old = SelectObject(dc, dib.into());
            let drew = DrawIconEx(dc, 0, 0, hicon, SIZE_PX, SIZE_PX, 0, None, DI_NORMAL).is_ok();
            let px = drew.then(|| std::slice::from_raw_parts(bits as *const u8, (SIZE_PX * SIZE_PX * 4) as usize).to_vec());
            SelectObject(dc, old);
            let _ = DeleteObject(dib.into());
            px
        });
        let _ = DeleteDC(dc);
        let mut px = out?;
        // GDI drawing may leave alpha at 0 for icons without an alpha channel.
        if px.chunks_exact(4).all(|p| p[3] == 0) {
            for p in px.chunks_exact_mut(4).filter(|p| p[..3] != [0, 0, 0]) {
                p[3] = 255;
            }
        }
        pixels_to_png_url(px, SIZE_PX as u32, SIZE_PX as u32)
    }
}

unsafe fn bitmap_to_png_url(bmp: HBITMAP) -> Option<String> {
    unsafe {
        let mut bm = BITMAP::default();
        if GetObjectW(bmp.into(), size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let rows = GetDIBits(dc, bmp, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        if rows == 0 {
            return None;
        }
        pixels_to_png_url(px, w as u32, h as u32)
    }
}

/// BGRA pixels in, PNG data URL out.
fn pixels_to_png_url(mut px: Vec<u8>, w: u32, h: u32) -> Option<String> {
    {
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2); // BGRA -> RGBA
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().ok()?.write_image_data(&px).ok()?;
        Some(format!("data:image/png;base64,{}", STANDARD.encode(out)))
    }
}
