//! Apps pinned to the real Windows taskbar: the shortcuts in `User Pinned\TaskBar`.
//! Read once at start. Ids match the tracker's group ids: the shortcut's AppUserModelID when it has one
//! ("aumid:<id>", also what running windows group under), else its target exe path.
use crate::config::Pin;
use std::{path::PathBuf, time::SystemTime};
use windows::{
    Win32::{
        Storage::EnhancedStorage::PKEY_AppUserModel_ID,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, STGM_READ,
            StructuredStorage::PropVariantToStringAlloc,
        },
        UI::Shell::{
            IShellLinkW, PropertiesSystem::{GPS_DEFAULT, IPropertyStore, SHGetPropertyStoreFromParsingName}, ShellLink,
        },
    },
    core::HSTRING,
};

fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"))
}

fn aumid(lnk: &str) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreFromParsingName(&HSTRING::from(lnk), None, GPS_DEFAULT).ok()?;
        let pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&pv as *const _).ok()?;
        let out = s.to_string().ok();
        CoTaskMemFree(Some(s.0 as *const _));
        out.filter(|s| !s.is_empty())
    }
}

fn target(lnk: &str) -> Option<String> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = windows::core::Interface::cast(&link).ok()?;
        file.Load(&HSTRING::from(lnk), STGM_READ).ok()?;
        let mut buf = [0u16; 520];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..n])).filter(|s| !s.is_empty())
    }
}

/// Cheap change marker for the pin folder: file names and modified times, no COM.
pub fn signature() -> String {
    let Some(rd) = dir().and_then(|d| std::fs::read_dir(d).ok()) else { return String::new() };
    let mut v: Vec<String> = rd
        .flatten()
        .map(|e| format!("{}:{:?}", e.file_name().to_string_lossy(), e.metadata().and_then(|m| m.modified()).ok()))
        .collect();
    v.sort();
    v.join("|")
}

/// Taskbar pins, oldest first (the folder keeps no order, pin time is the closest stand-in).
pub fn read() -> Vec<Pin> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let Some(rd) = dir().and_then(|d| std::fs::read_dir(d).ok()) else { return vec![] };
    let mut files: Vec<(SystemTime, String)> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path().to_string_lossy().into_owned())))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|(_, p)| aumid(&p).map(|a| format!("aumid:{a}")).or_else(|| target(&p)))
        .map(|id| Pin { id })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn live_read() {
        println!("{:#?}", super::read());
    }
}
