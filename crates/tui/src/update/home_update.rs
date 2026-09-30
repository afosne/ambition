//! 主页面状态机（Home 屏导航 + Enter 路由到各功能屏）
//! 跨域引用一律 `use crate::*`（docs/01 §1 组织规范）。

use crate::*;

/// 每个菜单项占用的行数（标题 + 描述 + 空行）
pub const LINES_PER_ITEM: usize = 3;
/// 标题区行数（2 行文本 + 1 行空行）
pub const HEADER_LINES: usize = 3;
/// 底部提示区预留行数
pub const FOOTER_LINES: usize = 3;

/// 主页面菜单项数（与 render::home_entries 顺序同源）
pub fn home_menu_len() -> usize {
    13
}

/// 菜单内容最大可滚动行数（内容总行数 - 视口高度）
/// 视口高度依赖终端尺寸，取保守下界 1 防止溢出
pub fn home_max_scroll() -> usize {
    let total = home_menu_len() * LINES_PER_ITEM;
    // 视口高度 = 终端高度 - 外框(2) - 标题区 - 底部提示区
    // 终端高度未知（纯函数层），用保守估计：至少能显示 1 项
    let viewport_min = LINES_PER_ITEM;
    total.saturating_sub(viewport_min)
}

/// 光标移动后确保光标在可见视口内（调整 home_scroll）
/// 使用渲染层回填的 home_menu_viewport；首帧（viewport=0）时不调整
pub fn home_ensure_cursor_visible(state: &mut AppState) {
    let vp = state.home_menu_viewport;
    if vp == 0 {
        return; // 首帧或视口未知，渲染层会兜底
    }
    let cursor_top = state.home_cursor * LINES_PER_ITEM;
    let cursor_bot = cursor_top + LINES_PER_ITEM;
    if cursor_top < state.home_scroll {
        // 光标在视口上方 → 上滚
        state.home_scroll = cursor_top;
    } else if cursor_bot > state.home_scroll + vp {
        // 光标在视口下方 → 下滚
        state.home_scroll = cursor_bot.saturating_sub(vp);
    }
    // 光标在视口内 → 不动（稳定，不跳动）
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
        11 => Screen::Cost,
        12 => Screen::Qpack,
        _ => return Cmd::None,
    };
    super::m6::update_screen(state, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_menu_len_is_thirteen() {
        assert_eq!(home_menu_len(), 13);
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

        // cursor=11 → Cost (CostLoad)
        s.screen = Screen::Home;
        s.home_cursor = 11;
        assert_eq!(home_enter(&mut s), Cmd::CostLoad);
        assert_eq!(s.screen, Screen::Cost);
    }

    // 配对桩：fn home_max_scroll → test_home_max_scroll（同文件配对门禁）
    #[test]
    fn test_home_max_scroll_returns_total_minus_min_viewport() {
        // 13 项 * 3 行 = 39 行内容；保守视口 = 3 行 → max = 36
        assert_eq!(home_max_scroll(), 36);
    }

    // 配对桩：fn home_ensure_cursor_visible → test_home_ensure_cursor_visible
    #[test]
    fn test_home_ensure_cursor_visible_adjusts_scroll_when_cursor_exits() {
        let mut s = new_state(vec![]);
        s.screen = Screen::Home;
        s.home_menu_viewport = 12; // 4 项可见

        // 光标在第 5 项（行 15-17），scroll=0 → 光标在视口下方 → 下滚
        s.home_cursor = 5;
        s.home_scroll = 0;
        home_ensure_cursor_visible(&mut s);
        assert_eq!(
            s.home_scroll, 6,
            "光标在下方 → scroll 到 cursor_top=15-12+3"
        );

        // 光标在第 0 项，scroll=9 → 光标在视口上方 → 上滚到 0
        s.home_cursor = 0;
        s.home_scroll = 9;
        home_ensure_cursor_visible(&mut s);
        assert_eq!(s.home_scroll, 0, "光标在上方 → scroll 到 cursor_top=0");

        // 光标在视口中间 → 不动
        s.home_cursor = 2;
        s.home_scroll = 3;
        home_ensure_cursor_visible(&mut s);
        assert_eq!(s.home_scroll, 3, "光标在视口内 → 不动");

        // viewport=0（首帧）→ 不调整
        s.home_menu_viewport = 0;
        s.home_scroll = 5;
        home_ensure_cursor_visible(&mut s);
        assert_eq!(s.home_scroll, 5, "viewport=0 → 不调整");
    }
}
