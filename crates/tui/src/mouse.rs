//! 鼠标可交互区域注册表（render 注册 → main 查表映射 Msg）
//!
//! 每帧渲染时 `clear_click_targets` 清空，各 render 函数在绘制元素时注册 Rect + Msg。
//! - **悬停**（`MouseEventKind::Moved`）→ `hit_test_hover` → 只移动光标，不触发动作
//! - **点击**（`MouseEventKind::Down(Left)`）→ `hit_test_click` → 移动光标 + 触发选中/进入
//!
//! 设计：`ClickTarget` 同时存 `hover_msg`（只移光标）和 `click_msg`（触发动作）。
//! `hover_msg = None` 的区域不支持悬停跟随（如帮助浮层关闭区）。

use crate::*;

/// 一个可交互目标：屏幕区域 + 悬停 Msg + 点击 Msg
#[derive(Debug, Clone, PartialEq)]
pub struct ClickTarget {
    pub area: Rect,
    /// 点击时触发的 Msg（选中/进入/翻面等动作）
    pub click_msg: Msg,
    /// 悬停时触发的 Msg（只移光标，不触发动作）；None = 不支持悬停跟随
    pub hover_msg: Option<Msg>,
}

impl AppState {
    /// 清空可交互区域注册表（每帧渲染开头调用）
    pub fn clear_click_targets(&mut self) {
        self.click_targets.clear();
    }

    /// 注册一个纯点击区域（无悬停跟随）
    pub fn register_click(&mut self, area: Rect, msg: Msg) {
        if area.width > 0 && area.height > 0 {
            self.click_targets.push(ClickTarget {
                area,
                click_msg: msg,
                hover_msg: None,
            });
        }
    }

    /// 注册一个交互区域（悬停跟随光标 + 点击触发动作）
    pub fn register_interactive(&mut self, area: Rect, hover_msg: Msg, click_msg: Msg) {
        if area.width > 0 && area.height > 0 {
            self.click_targets.push(ClickTarget {
                area,
                click_msg,
                hover_msg: Some(hover_msg),
            });
        }
    }

    /// 点击命中测试：返回匹配的 click Msg（从后往前，浮层优先）
    pub fn hit_test_click(&self, row: u16, column: u16) -> Option<Msg> {
        self.click_targets
            .iter()
            .rev()
            .find(|t| point_in_rect(column, row, t.area))
            .map(|t| t.click_msg)
    }

    /// 悬停命中测试：返回匹配的 hover Msg（从后往前，浮层优先）
    pub fn hit_test_hover(&self, row: u16, column: u16) -> Option<Msg> {
        self.click_targets
            .iter()
            .rev()
            .find(|t| t.hover_msg.is_some() && point_in_rect(column, row, t.area))
            .and_then(|t| t.hover_msg)
    }
}

/// 点是否在 Rect 内
fn point_in_rect(column: u16, row: u16, r: Rect) -> bool {
    column >= r.x && column < r.x + r.width && row >= r.y && row < r.y + r.height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_test_click_finds_registered_target() {
        // 配对桩：hit_test_click
        let mut s = new_state(vec![]);
        s.clear_click_targets();
        s.register_click(Rect::new(5, 3, 20, 1), Msg::Submit);
        assert_eq!(s.hit_test_click(3, 10), Some(Msg::Submit));
        assert_eq!(s.hit_test_click(3, 4), None, "列在区域外");
        assert_eq!(s.hit_test_click(2, 10), None, "行在区域外");
    }

    #[test]
    fn test_hit_test_click_later_target_wins() {
        let mut s = new_state(vec![]);
        s.clear_click_targets();
        s.register_click(Rect::new(0, 0, 80, 24), Msg::Quit);
        s.register_click(Rect::new(10, 5, 20, 1), Msg::Submit);
        assert_eq!(s.hit_test_click(5, 15), Some(Msg::Submit));
        assert_eq!(s.hit_test_click(0, 0), Some(Msg::Quit));
    }

    #[test]
    fn test_clear_click_targets_resets() {
        let mut s = new_state(vec![]);
        s.register_click(Rect::new(0, 0, 10, 1), Msg::Quit);
        s.clear_click_targets();
        assert_eq!(s.hit_test_click(0, 0), None);
        assert!(s.click_targets.is_empty());
    }

    #[test]
    fn test_register_interactive_registers_both_hover_and_click() {
        let mut s = new_state(vec![]);
        s.clear_click_targets();
        s.register_interactive(
            Rect::new(0, 0, 20, 1),
            Msg::HoverOption(1),
            Msg::ClickOption(1),
        );
        // 悬停 → HoverOption
        assert_eq!(s.hit_test_hover(0, 5), Some(Msg::HoverOption(1)));
        // 点击 → ClickOption
        assert_eq!(s.hit_test_click(0, 5), Some(Msg::ClickOption(1)));
    }

    #[test]
    fn test_register_click_only_target_no_hover() {
        let mut s = new_state(vec![]);
        s.clear_click_targets();
        s.register_click(Rect::new(0, 0, 20, 1), Msg::ToggleHelp);
        // 点击有 → 悬停无
        assert_eq!(s.hit_test_click(0, 5), Some(Msg::ToggleHelp));
        assert_eq!(s.hit_test_hover(0, 5), None);
    }

    #[test]
    fn test_hit_test_hover_priority_same_as_click() {
        let mut s = new_state(vec![]);
        s.clear_click_targets();
        s.register_interactive(
            Rect::new(0, 0, 80, 24),
            Msg::HoverHome(0),
            Msg::HomeClick(0),
        );
        s.register_interactive(
            Rect::new(10, 5, 20, 1),
            Msg::HoverHome(3),
            Msg::HomeClick(3),
        );
        // 重叠区域 → 后注册的优先
        assert_eq!(s.hit_test_hover(5, 15), Some(Msg::HoverHome(3)));
        assert_eq!(s.hit_test_hover(0, 0), Some(Msg::HoverHome(0)));
    }

    #[test]
    fn test_point_in_rect_inside_and_outside() {
        let r = Rect::new(5, 3, 10, 5); // x=5..14, y=3..7
        assert!(point_in_rect(5, 3, r), "左上角在内");
        assert!(point_in_rect(14, 7, r), "右下角在内");
        assert!(!point_in_rect(4, 3, r), "列在左外");
        assert!(!point_in_rect(15, 3, r), "列在右外");
        assert!(!point_in_rect(5, 2, r), "行在上外");
        assert!(!point_in_rect(5, 8, r), "行在下外");
    }
}
