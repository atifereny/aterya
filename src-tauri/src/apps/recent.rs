//! Recently opened files of an app (its jump list's "Recent" category), read through the shell's
//! IApplicationDocumentLists. Works for apps with an explicit AppUserModelID (Store apps, Office, Electron apps...).
use windows::{
    Win32::{
        System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree},
        UI::Shell::{
            ADLT_RECENT, ApplicationDocumentLists, Common::IObjectArray, IApplicationDocumentLists, IShellItem, IShellLinkW, SIGDN_FILESYSPATH,
            SIGDN_NORMALDISPLAY,
        },
    },
    core::{HSTRING, Interface, PWSTR},
};

/// One entry of the list: what to show, and how to open it.
#[derive(Clone, Debug, PartialEq)]
pub struct Recent {
    pub name: String,
    /// A file path (opened with its default app) or, for a shell link, the program to run.
    pub target: String,
    /// Arguments when the entry is a shell link.
    pub args: Option<String>,
}

fn take(p: PWSTR) -> String {
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as _)) };
    s
}

/// Up to `max` recent entries of the app with this dock id (`aumid:<id>`); empty for other ids.
pub fn list(id: &str, max: u32) -> Vec<Recent> {
    let Some(appid) = id.strip_prefix("aumid:") else { return vec![] };
    let mut out = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let Ok(lists) = CoCreateInstance::<_, IApplicationDocumentLists>(&ApplicationDocumentLists, None, CLSCTX_INPROC_SERVER) else { return out };
        if lists.SetAppID(&HSTRING::from(appid)).is_err() {
            return out;
        }
        let Ok(arr) = lists.GetList::<IObjectArray>(ADLT_RECENT, max) else { return out };
        for i in 0..arr.GetCount().unwrap_or(0).min(max) {
            if let Ok(item) = arr.GetAt::<IShellItem>(i) {
                let name = item.GetDisplayName(SIGDN_NORMALDISPLAY).map(take).unwrap_or_default();
                let path = item.GetDisplayName(SIGDN_FILESYSPATH).map(take).unwrap_or_default();
                if !path.is_empty() {
                    out.push(Recent { name: if name.is_empty() { path.rsplit('\\').next().unwrap_or(&path).to_string() } else { name }, target: path, args: None });
                }
            } else if let Ok(link) = arr.GetAt::<IShellLinkW>(i) {
                let mut path = [0u16; 520];
                let mut args = [0u16; 1024];
                let mut desc = [0u16; 260];
                let _ = link.GetPath(&mut path, std::ptr::null_mut(), 0);
                let _ = link.GetArguments(&mut args);
                let _ = link.GetDescription(&mut desc);
                let s = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]);
                let (target, a, d) = (s(&path), s(&args), s(&desc));
                if !target.is_empty() {
                    let name = if d.is_empty() { target.rsplit('\\').next().unwrap_or(&target).to_string() } else { d };
                    out.push(Recent { name, target, args: Some(a) });
                }
            }
        }
    }
    out
}

/// Open an entry: a file with its default app, or run the shell link's program with its arguments.
pub fn open(r: &Recent) {
    use std::os::windows::process::CommandExt;
    let mut c = match &r.args {
        Some(a) => {
            let mut c = std::process::Command::new(&r.target);
            c.raw_arg(a);
            c
        }
        None => {
            let mut c = std::process::Command::new("explorer.exe");
            c.arg(&r.target);
            c
        }
    };
    let _ = c.creation_flags(0x0800_0000).spawn(); // CREATE_NO_WINDOW
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn live_list() {
        println!("{:#?}", super::list("aumid:Microsoft.Windows.Explorer", 5));
    }
}
