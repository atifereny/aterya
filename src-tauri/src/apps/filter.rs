pub struct WindowInfo {
    pub visible: bool,
    pub has_owner: bool,
    pub tool_window: bool,
    pub app_window: bool,
    pub cloaked: bool,
    pub title: String,
}

/// Mirrors the shell rule for which top-level windows get a taskbar button.
pub fn is_taskbar_window(w: &WindowInfo) -> bool {
    w.visible
        && !w.cloaked
        && !w.tool_window
        && !w.title.is_empty()
        && (!w.has_owner || w.app_window)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> WindowInfo {
        WindowInfo { visible: true, has_owner: false, tool_window: false, app_window: false, cloaked: false, title: "Doc".into() }
    }

    #[test]
    fn visible_unowned_titled_passes() {
        assert!(is_taskbar_window(&base()));
    }
    #[test]
    fn cloaked_fails() {
        assert!(!is_taskbar_window(&WindowInfo { cloaked: true, ..base() }));
    }
    #[test]
    fn tool_window_fails() {
        assert!(!is_taskbar_window(&WindowInfo { tool_window: true, ..base() }));
    }
    #[test]
    fn owned_with_app_window_passes() {
        assert!(is_taskbar_window(&WindowInfo { has_owner: true, app_window: true, ..base() }));
        assert!(!is_taskbar_window(&WindowInfo { has_owner: true, ..base() }));
    }
    #[test]
    fn empty_title_fails() {
        assert!(!is_taskbar_window(&WindowInfo { title: String::new(), ..base() }));
    }
}
