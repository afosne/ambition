//! 键位唯一入口：`KeyCtx` 路由 `key_to_msg` + 同源提示行 `hint`（docs/03 §5.3，2026-09-27 统一重构）
//! 两层规则：①恒定层 `Ctrl+Q`/`Ctrl+C` 任何上下文退出（raw mode 无 SIGINT）；②上下文层按 `KeyCtx` 选表。
//! `KeyCtx` 由 `AppState::key_ctx()` 计算；main 不做任何键位改写（Enter→下一题也在表内）；
//! render 提示行必须取 `hint()`——键位文案与路由同一来源，禁止硬编码。

use super::*;

/// 键位上下文（键位表只此一份；新增屏 = 新增变体）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCtx {
    /// 选择题作答（未判分）：j/k/Space/1-4/q
    Choice,
    /// 填空待答：一切可打印字符进输入行；`editing`=是否编辑中（决定 Esc 语义）
    Fill { editing: bool },
    /// 已判分 · 待人工裁定：y/n 裁决，Enter=下一题
    Pending,
    /// 已判分：Enter=下一题
    Graded,
    /// 全部完成
    Finished,
}

/// 键位 → 消息（唯一键位表；docs/03 §5.3）
pub fn key_to_msg(ctx: KeyCtx, code: KeyCode, mods: KeyModifiers) -> Option<Msg> {
    // ① 恒定层：Ctrl+Q / Ctrl+C 退出（任何上下文，含填空编辑中）
    if mods.contains(KeyModifiers::CONTROL)
        && matches!(code, KeyCode::Char('q') | KeyCode::Char('c'))
    {
        return Some(Msg::Quit);
    }
    // ② Esc / Enter：语义按上下文分化
    match code {
        KeyCode::Esc => {
            return match ctx {
                KeyCtx::Fill { editing: true } => Some(Msg::StopEdit),
                _ => Some(Msg::Quit),
            };
        }
        KeyCode::Enter => {
            return Some(match ctx {
                KeyCtx::Choice | KeyCtx::Fill { .. } => Msg::Submit,
                KeyCtx::Pending | KeyCtx::Graded | KeyCtx::Finished => Msg::Next,
            });
        }
        _ => {}
    }
    // ③ 填空待答：一切可打印字符（含空格/;/q/数字）都是文本——打字即恢复编辑
    if matches!(ctx, KeyCtx::Fill { .. }) {
        return match code {
            KeyCode::Backspace => Some(Msg::Backspace),
            KeyCode::Char(c) => Some(Msg::Char(c)),
            KeyCode::Down => Some(Msg::MoveDown),
            KeyCode::Up => Some(Msg::MoveUp),
            _ => None,
        };
    }
    // ④ 作答 / 裁决 / 收尾表
    match (ctx, code) {
        (KeyCtx::Choice, KeyCode::Char(' ')) => Some(Msg::Select),
        (KeyCtx::Choice, KeyCode::Char('q')) => Some(Msg::Quit),
        (KeyCtx::Choice, KeyCode::Char('j')) | (KeyCtx::Choice, KeyCode::Down) => {
            Some(Msg::MoveDown)
        }
        (KeyCtx::Choice, KeyCode::Char('k')) | (KeyCtx::Choice, KeyCode::Up) => Some(Msg::MoveUp),
        (KeyCtx::Choice, KeyCode::Char(c @ '1'..='4')) => {
            Some(Msg::Confidence(c.to_digit(10).unwrap_or(1) as u8))
        }
        (KeyCtx::Pending, KeyCode::Char('y')) => Some(Msg::Arbitrate(true)),
        (KeyCtx::Pending, KeyCode::Char('n')) => Some(Msg::Arbitrate(false)),
        (KeyCtx::Pending | KeyCtx::Graded | KeyCtx::Finished, KeyCode::Char('q')) => {
            Some(Msg::Quit)
        }
        _ => None,
    }
}

/// 提示行文案（与路由同源；render 直接取用，禁止硬编码键位）
pub fn hint(ctx: KeyCtx) -> &'static str {
    match ctx {
        KeyCtx::Choice => " j/k 移动 · Space 选中 · Enter 提交 · 1-4 自评把握 · q 退出",
        KeyCtx::Fill { editing: true } => {
            " 打字作答 · 多空用 ; 分隔 · Enter 提交 · Esc 退出编辑（保留输入）· Ctrl+Q 退出"
        }
        KeyCtx::Fill { editing: false } => " 打字恢复编辑（已输入保留）· Enter 提交 · Esc 退出程序",
        KeyCtx::Pending => " Enter 下一题 · y 判对 / n 判错 · q 退出",
        KeyCtx::Graded => " Enter 下一题 · q 退出",
        KeyCtx::Finished => " q 退出",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode as K, KeyModifiers as M};

    fn mods() -> M {
        M::empty()
    }

    #[test]
    fn test_key_to_msg_quit_rules_unified() {
        let ctxs = [
            KeyCtx::Choice,
            KeyCtx::Fill { editing: true },
            KeyCtx::Fill { editing: false },
            KeyCtx::Pending,
            KeyCtx::Graded,
            KeyCtx::Finished,
        ];
        for ctx in ctxs {
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
            Some(Msg::Quit)
        );
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::Char('q'), mods()),
            Some(Msg::Quit)
        );
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: true }, K::Char('q'), mods()),
            Some(Msg::Char('q')),
            "填空态 q 是文本"
        );
    }

    #[test]
    fn test_key_to_msg_esc_scoped_by_context() {
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: true }, K::Esc, mods()),
            Some(Msg::StopEdit),
            "编辑中 Esc=退出编辑保留文本"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Fill { editing: false }, K::Esc, mods()),
            Some(Msg::Quit),
            "未编辑 Esc=退出程序"
        );
        for ctx in [
            KeyCtx::Choice,
            KeyCtx::Pending,
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
        for ctx in [KeyCtx::Pending, KeyCtx::Graded, KeyCtx::Finished] {
            assert_eq!(
                key_to_msg(ctx, K::Enter, mods()),
                Some(Msg::Next),
                "{ctx:?}"
            );
        }
    }

    #[test]
    fn test_key_to_msg_fill_swallows_all_printables() {
        let ctx = KeyCtx::Fill { editing: false };
        for c in ['a', 'q', '1', 'y', ' ', ';'] {
            assert_eq!(
                key_to_msg(ctx, K::Char(c), mods()),
                Some(Msg::Char(c)),
                "文本 {c:?}"
            );
        }
        assert_eq!(key_to_msg(ctx, K::Backspace, mods()), Some(Msg::Backspace));
        assert_eq!(key_to_msg(ctx, K::Tab, mods()), None, "未映射键忽略");
    }

    #[test]
    fn test_key_to_msg_choice_select_move_and_confidence() {
        let c = KeyCtx::Choice;
        assert_eq!(
            key_to_msg(c, K::Char(' '), mods()),
            Some(Msg::Select),
            "Space=选中"
        );
        assert_eq!(key_to_msg(c, K::Char('j'), mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(c, K::Down, mods()), Some(Msg::MoveDown));
        assert_eq!(key_to_msg(c, K::Char('k'), mods()), Some(Msg::MoveUp));
        assert_eq!(key_to_msg(c, K::Up, mods()), Some(Msg::MoveUp));
        for level in 1..=4 {
            assert_eq!(
                key_to_msg(c, K::Char(char::from(b'0' + level)), mods()),
                Some(Msg::Confidence(level)),
                "{level} 档自评"
            );
        }
        assert_eq!(key_to_msg(c, K::Char('5'), mods()), None, "5 档不存在");
        assert_eq!(key_to_msg(c, K::Char('y'), mods()), None, "非待裁定不裁决");
        assert_eq!(key_to_msg(c, K::Backspace, mods()), None, "作答态无退格");
    }

    #[test]
    fn test_key_to_msg_pending_only_arbitration_and_next() {
        let p = KeyCtx::Pending;
        assert_eq!(
            key_to_msg(p, K::Char('y'), mods()),
            Some(Msg::Arbitrate(true))
        );
        assert_eq!(
            key_to_msg(p, K::Char('n'), mods()),
            Some(Msg::Arbitrate(false))
        );
        assert_eq!(key_to_msg(p, K::Char('1'), mods()), None, "判分后不自评");
        assert_eq!(key_to_msg(p, K::Enter, mods()), Some(Msg::Next));
        assert_eq!(
            key_to_msg(KeyCtx::Graded, K::Char('y'), mods()),
            None,
            "非 Pending 不裁决"
        );
        assert_eq!(
            key_to_msg(KeyCtx::Finished, K::Char('q'), mods()),
            Some(Msg::Quit)
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
        ];
        for h in all {
            assert!(!h.is_empty(), "提示非空");
        }
        let mut uniq: Vec<&str> = all.to_vec();
        uniq.dedup();
        assert_eq!(uniq.len(), all.len(), "各上下文提示互异");
        assert!(hint(KeyCtx::Choice).contains("Space"), "选择提示含选中键");
        assert!(
            hint(KeyCtx::Fill { editing: true }).contains("Esc 退出编辑"),
            "填空编辑提示含退出编辑"
        );
        assert!(
            hint(KeyCtx::Pending).contains("y 判对"),
            "待裁定提示含裁决键"
        );
    }
}
