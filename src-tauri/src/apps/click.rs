#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupState {
    /// Most recently used first.
    pub windows: Vec<isize>,
    pub focused: Option<isize>,
}

#[derive(Debug, PartialEq)]
pub enum ClickAction {
    Launch,
    Focus(isize),
    Minimize(isize),
}

pub fn decide_click(g: &GroupState) -> ClickAction {
    let Some(&first) = g.windows.first() else { return ClickAction::Launch };
    match g.focused.and_then(|f| g.windows.iter().position(|&w| w == f)) {
        None => ClickAction::Focus(first),
        Some(_) if g.windows.len() == 1 => ClickAction::Minimize(first),
        Some(i) => ClickAction::Focus(g.windows[(i + 1) % g.windows.len()]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(windows: &[isize], focused: Option<isize>) -> GroupState {
        GroupState { windows: windows.to_vec(), focused }
    }

    #[test]
    fn no_windows_launches() {
        assert_eq!(decide_click(&g(&[], None)), ClickAction::Launch);
    }
    #[test]
    fn unfocused_focuses_first() {
        assert_eq!(decide_click(&g(&[1, 2], None)), ClickAction::Focus(1));
    }
    #[test]
    fn focused_single_minimizes() {
        assert_eq!(decide_click(&g(&[7], Some(7))), ClickAction::Minimize(7));
    }
    #[test]
    fn focused_cycles_to_next() {
        assert_eq!(decide_click(&g(&[1, 2, 3], Some(1))), ClickAction::Focus(2));
    }
    #[test]
    fn focused_last_wraps() {
        assert_eq!(decide_click(&g(&[1, 2, 3], Some(3))), ClickAction::Focus(1));
    }
}
