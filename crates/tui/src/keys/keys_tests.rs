//! 键位路由/提示的补充测试（M5-W4b 拆出；主体测试留 keys/mod.rs 配对，P-16）

#[cfg(test)]
mod tests {
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

    /// Esc 应返回主页面（仅 Home 屏 Esc 退出）
    fn esc_ctxs() -> [KeyCtx; 4] {
        [
            KeyCtx::Fill { editing: false },
            KeyCtx::Choice,
            KeyCtx::Graded,
            KeyCtx::Finished,
        ]
    }

    #[test]
    fn test_key_to_msg_translates_workbench_family() {
        use Msg::*;
        assert_eq!(key_to_msg(KeyCtx::Choice, K::Char('w'), M::CONTROL), None);
        assert_eq!(key_to_msg(KeyCtx::Tx, K::F(5), mods()), Some(TxAlign));
        assert_eq!(key_to_msg(KeyCtx::Tx, K::F(6), mods()), Some(TxMyTurnOpen));
        assert_eq!(key_to_msg(KeyCtx::Tx, K::F(7), mods()), Some(TxHistoryOpen));
        assert_eq!(
            key_to_msg(KeyCtx::Tx, K::Char('a'), mods()),
            Some(Char('a'))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Tx, K::Backspace, mods()),
            Some(Backspace)
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxHistory, K::Up, mods()),
            Some(TxHistoryUp)
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxHistory, K::Down, mods()),
            Some(TxHistoryDown)
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxHistory, K::Char(' '), mods()),
            Some(TxToggleFavorite)
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxMyTurn, K::Char('x'), mods()),
            Some(Char('x'))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Tx, K::Esc, mods()),
            Some(Screen(crate::Screen::Home))
        );
        assert_eq!(
            key_to_msg(KeyCtx::TxAlign, K::Esc, mods()),
            Some(Screen(crate::Screen::Tx))
        );
    }

    #[test]
    fn test_key_to_msg_quit_rules_unified() {
        for ctx in all_ctxs() {
            assert_eq!(key_to_msg(ctx, K::Char('q'), M::CONTROL), Some(Msg::Quit));
            assert_eq!(key_to_msg(ctx, K::Char('c'), M::CONTROL), None);
        }
        assert_eq!(key_to_msg(KeyCtx::Choice, K::Char('q'), mods()), None);
        assert_eq!(key_to_msg(KeyCtx::Graded, K::Char('q'), mods()), None);
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: true }, K::Char('q'), mods()),
            Some(Msg::Char('q'))
        );
    }

    #[test]
    fn test_key_to_msg_no_single_letter_shortcuts() {
        for ctx in [
            KeyCtx::Choice,
            KeyCtx::Pending,
            KeyCtx::Graded,
            KeyCtx::Finished,
        ] {
            for b in b'a'..=b'z' {
                assert_eq!(key_to_msg(ctx, K::Char(b as char), mods()), None);
            }
        }
        for c in ['q', 'j', 'k', 'y', 'n'] {
            assert_eq!(
                key_to_msg(KeyCtx::Fill { editing: false }, K::Char(c), mods()),
                Some(Msg::Char(c))
            );
        }
    }

    #[test]
    fn test_key_to_msg_esc_scoped_by_context() {
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: true }, K::Esc, mods()),
            Some(Msg::StopEdit)
        );
        assert_eq!(key_to_msg(KeyCtx::Pending, K::Esc, mods()), Some(Msg::Next));
        for ctx in esc_ctxs() {
            assert_eq!(
                key_to_msg(ctx, K::Esc, mods()),
                Some(Msg::Screen(crate::Screen::Home))
            );
        }
        assert_eq!(key_to_msg(KeyCtx::Home, K::Esc, mods()), Some(Msg::Quit));
    }

    #[test]
    fn test_key_to_msg_enter_unified_submit_vs_next() {
        for ctx in [
            KeyCtx::Choice,
            KeyCtx::Fill { editing: true },
            KeyCtx::Fill { editing: false },
        ] {
            assert_eq!(key_to_msg(ctx, K::Enter, mods()), Some(Msg::Submit));
        }
        assert_eq!(
            key_to_msg(KeyCtx::Pending, K::Enter, mods()),
            Some(Msg::ConfirmArbitrate)
        );
        for ctx in [KeyCtx::Graded, KeyCtx::Finished] {
            assert_eq!(key_to_msg(ctx, K::Enter, mods()), Some(Msg::Next));
        }
    }

    #[test]
    fn test_key_to_msg_fill_swallows_printables_and_navigates_blanks() {
        let ctx = KeyCtx::Fill { editing: false };
        for c in ['a', 'q', '1', 'y', ' ', ';'] {
            assert_eq!(key_to_msg(ctx, K::Char(c), mods()), Some(Msg::Char(c)));
        }
        assert_eq!(key_to_msg(ctx, K::Backspace, mods()), Some(Msg::Backspace));
        assert_eq!(key_to_msg(ctx, K::Tab, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(ctx, K::BackTab, mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(ctx, K::Right, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(ctx, K::Left, mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(ctx, K::F(5), mods()), None);
    }

    #[test]
    fn test_key_to_msg_choice_uses_arrows_space_and_digits() {
        let c = KeyCtx::Choice;
        assert_eq!(key_to_msg(c, K::Down, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(c, K::Up, mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(c, K::Char(' '), mods()), Some(Msg::Select));
        for level in 1..=4 {
            assert_eq!(
                key_to_msg(c, K::Char(char::from(b'0' + level)), mods()),
                None
            );
        }
        assert_eq!(key_to_msg(c, K::Char('5'), mods()), None);
        assert_eq!(key_to_msg(c, K::Backspace, mods()), None);
    }

    #[test]
    fn test_key_to_msg_pending_arrows_select_and_enter_confirms() {
        let p = KeyCtx::Pending;
        assert_eq!(key_to_msg(p, K::Right, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(p, K::Down, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(p, K::Left, mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(p, K::Up, mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(p, K::Enter, mods()), Some(Msg::ConfirmArbitrate));
        assert_eq!(key_to_msg(p, K::Char('1'), mods()), None);
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::Right, mods()),
            Some(Msg::Skip)
        );
    }

    #[test]
    fn test_key_to_msg_navigation_and_picker_keys() {
        use Msg::*;
        for ctx in [KeyCtx::Choice, KeyCtx::Graded] {
            assert_eq!(key_to_msg(ctx, K::Right, mods()), Some(Skip));
            assert_eq!(key_to_msg(ctx, K::Left, mods()), Some(Prev));
        }
        for ctx in [KeyCtx::Choice, KeyCtx::Graded, KeyCtx::Finished] {
            assert_eq!(key_to_msg(ctx, K::F(1), mods()), Some(TogglePicker));
        }
        assert_eq!(
            key_to_msg(KeyCtx::Picker, K::F(1), mods()),
            Some(TogglePicker)
        );
        assert_eq!(key_to_msg(KeyCtx::Picker, K::Down, mods()), Some(MoveDown));
        assert_eq!(key_to_msg(KeyCtx::Picker, K::Up, mods()), Some(MoveUp));
        assert_eq!(key_to_msg(KeyCtx::Picker, K::Enter, mods()), Some(Jump));
        assert_eq!(
            key_to_msg(KeyCtx::Picker, K::Esc, mods()),
            Some(TogglePicker)
        );
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: false }, K::F(1), mods()),
            None
        );
    }

    #[test]
    fn test_key_to_msg_review_ctx_flip_rate_and_return() {
        use Msg::*;
        assert_eq!(key_to_msg(KeyCtx::Review, K::Char(' '), mods()), Some(Flip));
        assert_eq!(key_to_msg(KeyCtx::Review, K::Enter, mods()), Some(Flip));
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Char('1'), mods()),
            Some(Rate(1))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Char('4'), mods()),
            Some(Rate(4))
        );
        assert_eq!(key_to_msg(KeyCtx::Review, K::Char('5'), mods()), None);
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Tab, mods()),
            Some(Screen(crate::Screen::Home))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Esc, mods()),
            Some(Screen(crate::Screen::Home))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Char('q'), M::CONTROL),
            Some(Quit)
        );
    }

    #[test]
    fn test_key_to_msg_ctrl_direct_screen_navigation() {
        use Msg::*;
        for ctx in all_ctxs() {
            assert_eq!(
                key_to_msg(ctx, K::Char('a'), M::CONTROL),
                Some(Screen(crate::Screen::Practice))
            );
            assert_eq!(
                key_to_msg(ctx, K::Char('r'), M::CONTROL),
                Some(Screen(crate::Screen::Review))
            );
        }
        for c in ['w', 'n', 'd', 'k', 'p', 'g', 'v', 'o', 'x'] {
            assert_eq!(key_to_msg(KeyCtx::Home, K::Char(c), M::CONTROL), None);
        }
        assert_eq!(key_to_msg(KeyCtx::Home, K::Char('s'), M::CONTROL), None);
    }

    #[test]
    fn test_hint_covers_review_ctx_uniquely() {
        let hint_review = hint(KeyCtx::Review);
        assert!(hint_review.contains("翻面") && hint_review.contains("1-4"));
        for ctx in [KeyCtx::Choice, KeyCtx::Graded, KeyCtx::Finished] {
            assert_ne!(hint(ctx), hint_review);
        }
    }

    #[test]
    fn test_key_to_msg_scaffold_family() {
        use Msg::*;
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::F(2), mods()),
            Some(Screen(crate::Screen::Scaffold))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Esc, mods()),
            Some(Screen(crate::Screen::Home))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Enter, mods()),
            Some(ScaffoldRender)
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::F(3), mods()),
            Some(ScaffoldCheck)
        );
        assert_eq!(key_to_msg(KeyCtx::Choice, K::Char('s'), M::CONTROL), None);
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Tab, mods()),
            Some(ScaffoldSlotNext)
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Char('a'), mods()),
            Some(Char('a'))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Scaffold, K::Char(' '), mods()),
            Some(ScaffoldStage)
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
            assert!(!h.is_empty());
            assert!(!h.contains("j/k"));
        }
        let mut uniq: Vec<&str> = all.to_vec();
        uniq.dedup();
        assert_eq!(uniq.len(), all.len());
        assert!(hint(KeyCtx::Choice).contains("Space"));
        assert!(hint(KeyCtx::Choice).contains("↑/↓"));
        assert!(hint(KeyCtx::Fill { editing: true }).contains("Esc 退出编辑"));
        assert!(hint(KeyCtx::Pending).contains("判对"));
        assert!(hint(KeyCtx::Home).contains("Esc"));
        assert!(hint(KeyCtx::Home).contains("退出"));
        assert!(!hint(KeyCtx::Qpack).contains("Ctrl+O"));
        assert!(!hint(KeyCtx::Workspace).contains("Ctrl+N"));
    }

    #[test]
    fn test_key_to_msg_f4_annotates_only_in_graded_ctx() {
        use Msg::*;
        assert_eq!(key_to_msg(KeyCtx::Graded, K::F(4), mods()), Some(Annotate));
        assert_eq!(key_to_msg(KeyCtx::Choice, K::F(4), mods()), None);
        assert_eq!(key_to_msg(KeyCtx::Graded, K::Char('e'), mods()), None);
        assert_eq!(key_to_msg(KeyCtx::Graded, K::Char('e'), M::CONTROL), None);
        assert_eq!(key_to_msg(KeyCtx::Choice, K::Char('e'), M::CONTROL), None);
    }

    #[test]
    fn test_key_to_msg_ctrl_m_opens_mock_from_practice_ctxs() {
        use Msg::*;
        assert_eq!(
            key_to_msg(KeyCtx::Choice, K::Char('m'), M::CONTROL),
            Some(MockStart)
        );
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::Char('m'), M::CONTROL),
            Some(MockStart)
        );
        assert_eq!(key_to_msg(KeyCtx::Review, K::Char('m'), M::CONTROL), None);
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
            assert_eq!(key_to_msg(ctx, K::Char('?'), M::empty()), Some(ToggleHelp));
        }
    }
}
