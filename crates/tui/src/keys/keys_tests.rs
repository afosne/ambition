//! 键位路由/提示的补充测试（M5-W4b 拆出；主体测试留 keys/mod.rs 配对，P-16）

#[cfg(test)]
mod tests {
    #[test]
    fn test_key_to_msg_translates_workbench_family() {
        use crate::Screen;
        use Msg::*;
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::Char('w'), M::CONTROL),
            Some(Screen(Screen::Tx)),
            "Ctrl+W 屏幕直达（直达表 W 保留给工作台）"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Tx, K::F(5), mods()),
            Some(TxAlign),
            "F5 对齐"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Tx, K::F(6), mods()),
            Some(TxMyTurnOpen),
            "F6 我来翻"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Tx, K::F(7), mods()),
            Some(TxHistoryOpen),
            "F7 历史"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Tx, K::Char('a'), mods()),
            Some(Char('a')),
            "字符直入源文"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Tx, K::Backspace, mods()),
            Some(Backspace),
            "退格"
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxHistory, K::Up, mods()),
            Some(TxHistoryUp),
            "历史↑"
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxHistory, K::Down, mods()),
            Some(TxHistoryDown),
            "历史↓"
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxHistory, K::Char(' '), mods()),
            Some(TxToggleFavorite),
            "历史 Space=收藏"
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxMyTurn, K::Char('x'), mods()),
            Some(Char('x')),
            "我来翻字符入译文"
        );
    }

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
            key_to_msg(KeyCtx::Scaffold, K::F(3), mods()),
            Some(ScaffoldCheck),
            "F3 自查（动作键 F 区）"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::Char('s'), M::CONTROL),
            None,
            "Ctrl+S 保留 Settings 直达（docs/03 §5.3）"
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

    #[test]
    fn test_key_to_msg_f4_annotates_only_in_graded_ctx() {
        use Msg::*;
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::F(4), mods()),
            Some(Annotate),
            "判分屏 F4 → 批注（动作键走 F 区）"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::F(4), mods()),
            None,
            "非判分屏无操作"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::Char('e'), mods()),
            None,
            "单字母 e 不批注（禁单字母快捷键）"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::Char('e'), M::CONTROL),
            None,
            "Ctrl+E 保留 Writing 直达（docs/03 §5.3）"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::Char('e'), M::CONTROL),
            None,
            "Ctrl+E 在任何答题态都保留给 Writing"
        );
    }

    #[test]
    fn test_key_to_msg_ctrl_m_opens_mock_from_practice_ctxs() {
        use Msg::*;
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::Char('m'), M::CONTROL),
            Some(MockStart),
            "答题态开卷"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::Char('m'), M::CONTROL),
            Some(MockStart),
            "判分态开卷"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Char('m'), M::CONTROL),
            None,
            "非答题态不开卷"
        );
    }
}
