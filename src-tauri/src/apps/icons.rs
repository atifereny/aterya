use base64::{Engine, engine::general_purpose::STANDARD};
use std::{collections::HashMap, sync::Mutex};
use windows::{
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::*,
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY},
    },
    core::HSTRING,
};

static CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
const SIZE_PX: i32 = 32;

/// PNG data URL for a group, cached by group id. Empty string when no icon could be had.
/// `id` is an exe path or `aumid:<AUMID>`. Needs COM initialised on the calling thread.
// shortcut: the shell image factory serves Win32 exes and packaged apps alike, so no
// WM_GETICON/GCLP_HICON path; upgrade if an app's window icon must win over its exe icon.
pub fn icon_for(id: &str) -> String {
    if let Some(hit) = CACHE.lock().unwrap().get_or_insert_with(Default::default).get(id) {
        return hit.clone();
    }
    let url = shell_icon(id).unwrap_or_default();
    if !url.is_empty() {
        CACHE.lock().unwrap().get_or_insert_with(Default::default).insert(id.into(), url.clone());
    }
    url
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
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2); // BGRA -> RGBA
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().ok()?.write_image_data(&px).ok()?;
        Some(format!("data:image/png;base64,{}", STANDARD.encode(out)))
    }
}
