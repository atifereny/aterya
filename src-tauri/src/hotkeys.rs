//! Win+1 ... Win+0 open the nth dock app: the Windows taskbar's own shortcut, which the hidden taskbar no longer answers.
//! A low-level keyboard hook swallows the combination so Windows (and the Start menu) never sees it.
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LWIN,
            VK_MENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL,
            WM_KEYDOWN, WM_SYSKEYDOWN,
        },
    },
};

static WIN_DOWN: AtomicBool = AtomicBool::new(false);

fn held(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

/// A harmless key tap while Win is held: Windows then does not open the Start menu when Win is released.
fn cancel_start_menu() {
    let k = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0xE8), dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() }, ..Default::default() } },
    };
    unsafe { SendInput(&[k(false), k(true)], size_of::<INPUT>() as i32) };
}

unsafe extern "system" fn hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if k.flags.0 & LLKHF_INJECTED.0 == 0 {
            let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let vk = k.vkCode;
            if vk == VK_LWIN.0 as u32 || vk == VK_RWIN.0 as u32 {
                WIN_DOWN.store(down, Ordering::Relaxed);
            } else if WIN_DOWN.load(Ordering::Relaxed) && (0x30..=0x39).contains(&vk) && !held(VK_SHIFT) && !held(VK_CONTROL) && !held(VK_MENU) {
                let n = if vk == 0x30 { 9 } else { (vk - 0x31) as usize };
                if let Some(id) = crate::apps::tracker::nth_id(n) {
                    if down {
                        cancel_start_menu();
                        tauri::async_runtime::spawn(async move {
                            let _ = crate::apps::pins::app_click(id).await;
                        });
                    }
                    return LRESULT(1);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

pub fn start() {
    std::thread::spawn(|| unsafe {
        if let Err(e) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), None, 0) {
            return log::warn!("[hotkeys] keyboard hook failed: {e}");
        }
        log::info!("[hotkeys] Win+1..0 active");
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    });
}
