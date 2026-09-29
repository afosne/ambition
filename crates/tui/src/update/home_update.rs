//! 主页面状态机（Home 屏导航 + Enter 路由到各功能屏）
//! 跨域引用一律 `use crate::*`（docs/01 §1 组织规范）。

use crate::*;

/// 主页面菜单项数（与 render::home_entries 顺序同源）
pub fn home_menu_len() -> usize {
    11
}

/// 主页面 Enter：根据 home_cursor 切到对应功能屏
pub fn home_enter(state: &mut AppState) -> Cmd {
    let target = match state.home_cursor {
        0 => return Cmd::PracticeLoad, // 练习 → main 加载题目
        1 => Screen::Review,
        2 => Screen::Scaffold,
        3 => Screen::Mock,
        4 => Screen::Tx,
        5 => Screen::Notes,
        6 => Screen::Diary,
        7 => Screen::Todos,
        8 => Screen::Profile,
        9 => Screen::Games,
        10 => Screen::Wordbook,
        _ => return Cmd::None,
    };
    super::m6::update_screen(state, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_menu_len_is_eleven() {
        assert_eq!(home_menu_len(), 11);
    }

    #[test]
    fn test_home_enter_routes_to_expected_screens() {
        // cursor=0 → PracticeLoad
        let mut s = new_state(vec![]);
        s.screen = Screen::Home;
        s.home_cursor = 0;
        assert_eq!(home_enter(&mut s), Cmd::PracticeLoad);

        // cursor=1 → Review
        s.home_cursor = 1;
        assert_eq!(home_enter(&mut s), Cmd::ReviewLoad);
        assert_eq!(s.screen, Screen::Review);

        // cursor=5 → Notes (WorkspaceLoad)
        s.screen = Screen::Home;
        s.home_cursor = 5;
        assert_eq!(home_enter(&mut s), Cmd::WorkspaceLoad);
        assert_eq!(s.screen, Screen::Notes);

        // cursor=10 → Wordbook (VocabLoad)
        s.screen = Screen::Home;
        s.home_cursor = 10;
        assert_eq!(home_enter(&mut s), Cmd::VocabLoad);
        assert_eq!(s.screen, Screen::Wordbook);
    }
}
