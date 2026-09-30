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

    fn all_ctxs() -> [KeyCtx; 10] {
        [
            KeyCtx::Home,
            KeyCtx::Choice,
            KeyCtx::Fill { editing: true },
            KeyCtx::Fill { editing: false },
            KeyCtx::Pending,
            KeyCtx::Graded,
            KeyCtx::Finished,
            KeyCtx::Review,
            KeyCtx::Picker,
            KeyCtx::Scaffold,
        ]
    }

    #[test]
    fn test_key_to_msg_quit_rules_unified() {
        for ctx in all_ctxs() {
            assert_eq!(
                key_to_msg(ctx, K::Char('q'), M::CONTROL),
                Some(Msg::Quit),
                "Ctrl+Q 恒退出: {ctx:?}"
            );
            assert_eq!(
                key_to_msg(ctx, K::Char('c'), M::CONTROL),
                Some(Msg::Quit),
                "Ctrl+C 恒退出: {ctx:?}"
            );
        }
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::Char('q'), mods()),
            None,
            "单字母 q 不再退出（禁单字母快捷键）"
        );
        assert_eq!(key_to_msg(KeyCtx::Graded, K::Char('q'), mods()), None);
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: true }, K::Char('q'), mods()),
            Some(Msg::Char('q')),
            "填空态 q 是文本"
        );
    }

    #[test]
    fn test_key_to_msg_no_single_letter_shortcuts() {
        // 键位总则（docs/03 §5.3）：除填空打字外，a-z 一律不是命令
        for ctx in [
            KeyCtx::Choice,
            KeyCtx::Pending,
            KeyCtx::Graded,
            KeyCtx::Finished,
        ] {
            for b in b'a'..=b'z' {
                assert_eq!(
                    key_to_msg(ctx, K::Char(b as char), mods()),
                    None,
                    "{ctx:?} 禁单字母快捷键: {}",
                    b as char
                );
            }
        }
        // 填空：字母 = 打字文本（路由为 Char，由 update 落当前空位）
        for c in ['q', 'j', 'k', 'y', 'n'] {
            assert_eq!(
                key_to_msg(KeyCtx::Fill { editing: false }, K::Char(c), mods()),
                Some(Msg::Char(c)),
                "填空字母=打字 {c}"
            );
        }
    }

    #[test]
    fn test_key_to_msg_esc_scoped_by_context() {
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: true }, K::Esc, mods()),
            Some(Msg::StopEdit),
            "编辑中 Esc=退出编辑保留文本"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Pending, K::Esc, mods()),
            Some(Msg::Next),
            "待裁定 Esc=跳过（保持待裁定桶）"
        );
        for ctx in [
            KeyCtx::Fill { editing: false },
            KeyCtx::Choice,
            KeyCtx::Graded,
            KeyCtx::Finished,
        ] {
            assert_eq!(key_to_msg(ctx, K::Esc, mods()), Some(Msg::Quit), "{ctx:?}");
        }
    }

    #[test]
    fn test_key_to_msg_enter_unified_submit_vs_next() {
        for ctx in [
            KeyCtx::Choice,
            KeyCtx::Fill { editing: true },
            KeyCtx::Fill { editing: false },
        ] {
            assert_eq!(
                key_to_msg(ctx, K::Enter, mods()),
                Some(Msg::Submit),
                "{ctx:?}"
            );
        }
        assert_eq!(
            key_to_msg(KeyCtx::Pending, K::Enter, mods()),
            Some(Msg::ConfirmArbitrate),
            "待裁定 Enter=确认裁决"
        );
        for ctx in [KeyCtx::Graded, KeyCtx::Finished] {
            assert_eq!(
                key_to_msg(ctx, K::Enter, mods()),
                Some(Msg::Next),
                "{ctx:?}"
            );
        }
    }

    #[test]
    fn test_key_to_msg_fill_swallows_printables_and_navigates_blanks() {
        let ctx = KeyCtx::Fill { editing: false };
        for c in ['a', 'q', '1', 'y', ' ', ';'] {
            assert_eq!(
                key_to_msg(ctx, K::Char(c), mods()),
                Some(Msg::Char(c)),
                "文本 {c:?}"
            );
        }
        assert_eq!(key_to_msg(ctx, K::Backspace, mods()), Some(Msg::Backspace));
        assert_eq!(
            key_to_msg(ctx, K::Tab, mods()),
            Some(Msg::MoveDown),
            "Tab=下一空"
        );
        assert_eq!(
            key_to_msg(ctx, K::BackTab, mods()),
            Some(Msg::MoveUp),
            "Shift+Tab=上一空"
        );
        assert_eq!(key_to_msg(ctx, K::Right, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(ctx, K::Left, mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(ctx, K::F(5), mods()), None, "未映射键忽略");
    }

    #[test]
    fn test_key_to_msg_choice_uses_arrows_space_and_digits() {
        let c = KeyCtx::Choice;
        assert_eq!(key_to_msg(c, K::Down, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(c, K::Up, mods()), Some(Msg::MoveUp));
        assert_eq!(
            key_to_msg(c, K::Char(' '), mods()),
            Some(Msg::Select),
            "Space=选中"
        );
        for level in 1..=4 {
            assert_eq!(
                key_to_msg(c, K::Char(char::from(b'0' + level)), mods()),
                Some(Msg::Confidence(level)),
                "{level} 档自评（数字非字母）"
            );
        }
        assert_eq!(key_to_msg(c, K::Char('5'), mods()), None, "5 档不存在");
        assert_eq!(key_to_msg(c, K::Backspace, mods()), None, "作答态无退格");
    }

    #[test]
    fn test_key_to_msg_pending_arrows_select_and_enter_confirms() {
        let p = KeyCtx::Pending;
        assert_eq!(key_to_msg(p, K::Right, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(p, K::Down, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(p, K::Left, mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(p, K::Up, mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(p, K::Enter, mods()), Some(Msg::ConfirmArbitrate));
        assert_eq!(key_to_msg(p, K::Char('1'), mods()), None, "裁决态无自评");
        // 判分后 → = 跳过/下一题（2026-09-27 翻题键新契约；裁决态箭头仍归高亮）
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::Right, mods()),
            Some(Msg::Skip),
            "已判分 → 下一题"
        );
    }

    #[test]
    fn test_key_to_msg_navigation_and_picker_keys() {
        use Msg::*;
        // 翻题：→ Skip / ← Prev（Choice 与 Graded）
        for ctx in [KeyCtx::Choice, KeyCtx::Graded] {
            assert_eq!(key_to_msg(ctx, K::Right, mods()), Some(Skip), "{ctx:?} →");
            assert_eq!(key_to_msg(ctx, K::Left, mods()), Some(Prev), "{ctx:?} ←");
        }
        // F1 面板开合
        for ctx in [KeyCtx::Choice, KeyCtx::Graded, KeyCtx::Finished] {
            assert_eq!(
                key_to_msg(ctx, K::F(1), mods()),
                Some(TogglePicker),
                "F1 开面板: {ctx:?}"
            );
        }
        assert_eq!(
            key_to_msg(KeyCtx::Picker, K::F(1), mods()),
            Some(TogglePicker),
            "面板内 F1 关闭"
        );
        assert_eq!(key_to_msg(KeyCtx::Picker, K::Down, mods()), Some(MoveDown));
        assert_eq!(key_to_msg(KeyCtx::Picker, K::Up, mods()), Some(MoveUp));
        assert_eq!(key_to_msg(KeyCtx::Picker, K::Enter, mods()), Some(Jump));
        assert_eq!(
            key_to_msg(KeyCtx::Picker, K::Esc, mods()),
            Some(TogglePicker),
            "面板 Esc 关闭"
        );
        // 填空态 F1 不开面板（键位归文本/切空位）
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: false }, K::F(1), mods()),
            None
        );
    }

    #[test]
    fn test_key_to_msg_review_ctx_flip_rate_and_return() {
        use Msg::*;
        // 翻面：Space / Enter
        assert_eq!(key_to_msg(KeyCtx::Review, K::Char(' '), mods()), Some(Flip));
        assert_eq!(key_to_msg(KeyCtx::Review, K::Enter, mods()), Some(Flip));
        // 评分 1-4
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Char('1'), mods()),
            Some(Rate(1))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Char('4'), mods()),
            Some(Rate(4))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Char('5'), mods()),
            None,
            "5 档不存在"
        );
        // 返回主页面：Tab / Esc
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Tab, mods()),
            Some(Screen(crate::Screen::Home))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Esc, mods()),
            Some(Screen(crate::Screen::Home)),
            "Esc 返回主页面"
        );
        // Ctrl+Q 仍恒退出
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Char('q'), M::CONTROL),
            Some(Quit)
        );
    }

    #[test]
    fn test_key_to_msg_ctrl_direct_screen_navigation() {
        use Msg::*;
        // 恒定层 Ctrl+A / Ctrl+R 屏幕直达（docs/03 §5.3）
        for ctx in all_ctxs() {
            assert_eq!(
                key_to_msg(ctx, K::Char('a'), M::CONTROL),
                Some(Screen(crate::Screen::Practice)),
                "Ctrl+A → Practice: {ctx:?}"
            );
            assert_eq!(
                key_to_msg(ctx, K::Char('r'), M::CONTROL),
                Some(Screen(crate::Screen::Review)),
                "Ctrl+R → Review: {ctx:?}"
            );
        }
        assert_eq!(key_to_msg(KeyCtx::Choice, K::Char('x'), M::CONTROL), None);
    }

    #[test]
    fn test_hint_covers_review_ctx_uniquely() {
        let hint_review = hint(KeyCtx::Review);
        assert!(hint_review.contains("翻面") && hint_review.contains("1-4"));
        for ctx in [KeyCtx::Choice, KeyCtx::Graded, KeyCtx::Finished] {
            assert_ne!(hint(ctx), hint_review, "提示互异 {ctx:?}");
        }
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
            Some(Screen(crate::Screen::Home)),
            "Esc 返回主页面"
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
            hint(KeyCtx::Cost),
            hint(KeyCtx::Qpack),
        ];
        for h in all {
            assert!(!h.is_empty(), "提示非空");
            assert!(!h.contains("j/k"), "提示不含已废单字母: {h}");
        }
        let mut uniq: Vec<&str> = all.to_vec();
        uniq.dedup();
        assert_eq!(uniq.len(), all.len(), "各上下文提示互异");
        assert!(hint(KeyCtx::Choice).contains("Space"));
        assert!(hint(KeyCtx::Choice).contains("↑/↓"));
        assert!(hint(KeyCtx::Fill { editing: true }).contains("Esc 退出编辑"));
        assert!(hint(KeyCtx::Fill { editing: true }).contains("下一空"));
        assert!(hint(KeyCtx::Pending).contains("判对"));
        assert!(hint(KeyCtx::Pending).contains("Enter 确认"));
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

    /// #24：? 键触发 ToggleHelp
    #[test]
    fn test_key_to_msg_help_overlay_any_context() {
        use Msg::*;
        for ctx in [
            KeyCtx::Choice,
            KeyCtx::Pending,
            KeyCtx::Graded,
            KeyCtx::Finished,
            KeyCtx::Review,
            KeyCtx::Scaffold,
            KeyCtx::Tx,
            KeyCtx::Workspace,
        ] {
            assert_eq!(
                key_to_msg(ctx, K::Char('?'), M::empty()),
                Some(ToggleHelp),
                "? 在 {ctx:?} 应触发 ToggleHelp"
            );
        }
    }
}
