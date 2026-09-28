//! 键位路由/提示的补充测试（M5-W4b 拆出；主体测试留 keys/mod.rs 配对，P-16）

#[cfg(test)]
mod tests {
    use super::super::*;
    use crossterm::event::{KeyCode as K, KeyModifiers as M};

    fn mods() -> M {
        M::empty()
    }

    #[test]
    fn test_key_to_msg_scaffold_family() {
        use Msg::*;
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::F(2), mods()),
            Some(Screen(crate::Screen::Scaffold)),
            "F2 呼出模板屏"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Esc, mods()),
            Some(Screen(crate::Screen::Practice)),
            "Esc 返回答题"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Enter, mods()),
            Some(ScaffoldRender)
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Char('s'), M::CONTROL),
            Some(ScaffoldCheck),
            "Ctrl+S 自查"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::Char('s'), M::CONTROL),
            None,
            "非模板屏 Ctrl+S 无操作"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Tab, mods()),
            Some(ScaffoldSlotNext)
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Char('a'), mods()),
            Some(Char('a')),
            "字符 = 槽位文本"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Char(' '), mods()),
            Some(ScaffoldStage),
            "Space 切阶段"
        );
    }

    #[test]
    fn test_hint_matches_context_and_differs() {
        let all = [
            hint(KeyCtx::Choice),
            hint(KeyCtx::Fill { editing: true }),
            hint(KeyCtx::Fill { editing: false }),
            hint(KeyCtx::Pending),
            hint(KeyCtx::Graded),
            hint(KeyCtx::Finished),
            hint(KeyCtx::Scaffold),
        ];
        for h in all {
            assert!(!h.is_empty(), "提示非空");
            assert!(!h.contains("j/k"), "提示不含已废单字母: {h}");
        }
        let mut uniq: Vec<&str> = all.to_vec();
        uniq.dedup();
        assert_eq!(uniq.len(), all.len(), "各上下文提示互异");
        assert!(hint(KeyCtx::Choice).contains("Space"), "选择提示含选中键");
        assert!(hint(KeyCtx::Choice).contains("↑/↓"), "选择提示用方向键");
        assert!(
            hint(KeyCtx::Fill { editing: true }).contains("Esc 退出编辑"),
            "填空编辑提示含退出编辑"
        );
        assert!(
            hint(KeyCtx::Fill { editing: true }).contains("下一空"),
            "填空提示含空位导航"
        );
        assert!(hint(KeyCtx::Pending).contains("判对"), "待裁定提示含判对");
        assert!(
            hint(KeyCtx::Pending).contains("Enter 确认"),
            "待裁定提示含确认"
        );
    }
}
