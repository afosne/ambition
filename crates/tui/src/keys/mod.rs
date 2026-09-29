//! 键位唯一入口：`KeyCtx` 路由 `key_to_msg` + 同源提示行 `hint`（docs/03 §5.3，2026-09-27 统一重构）
//! 两层规则：①恒定层 `Ctrl+Q`/`Ctrl+C` 任何上下文退出（raw mode 无 SIGINT）；②上下文层按 `KeyCtx` 选表。
//! `KeyCtx` 由 `AppState::key_ctx()` 计算；main 不做任何键位改写（Enter→下一题也在表内）；
//! render 提示行必须取 `hint()`——键位文案与路由同一来源，禁止硬编码。

use super::*;

mod hints;
pub use hints::hint;

/// 键位上下文（键位表只此一份；新增屏 = 新增变体）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCtx {
    /// 选择题作答（未判分）：↑/↓/Space/1-4（单字母已废，docs/03 §5.3）
    Choice,
    /// 填空待答：一切可打印字符进输入行；`editing`=是否编辑中（决定 Esc 语义）
    Fill {
        editing: bool,
    },
    /// 已判分 · 待人工裁定：y/n 裁决，Enter=下一题
    Pending,
    /// 已判分：Enter=下一题
    Graded,
    /// 全部完成
    Finished,
    /// 复习屏（FSRS 翻卡；docs/03 §3 M4）：Space/Enter 翻面 · 1-4 评分 · Tab/Esc 返回
    Review,
    /// 选题面板（F1；docs/03 §5.3 2026-09-27）：↑↓ 选择 · Enter 跳转 · Esc/F1 返回
    Picker,
    /// 模板作文屏（F2，M5-W4b）：↑↓选模板 · Tab换槽 · Space阶段 · Enter成文 · Ctrl+S自查 · Esc 返回
    Scaffold,
    /// 翻译工作台主屏（M5-W7：源文输入与译文动作）
    Tx,
    /// 对照对齐屏（只读切段）
    TxAlign,
    /// 我来翻屏（输入我的译文）
    TxMyTurn,
    /// 翻译历史屏（选择/收藏/转练习）
    TxHistory,
    /// M6-W1 笔记/日记/TODO 编辑工作区
    Workspace,
    Profile,
    Game,
    Wordbook,
}

/// 键位 → 消息（唯一键位表；docs/03 §5.3）
pub fn key_to_msg(ctx: KeyCtx, code: KeyCode, mods: KeyModifiers) -> Option<Msg> {
    // ① 恒定层：Ctrl 组合（任何上下文）——Ctrl+Q/Ctrl+C 退出；其余 Ctrl+字母 = 屏幕直达
    //    （docs/03 §5.3：Ctrl+A/Ctrl+R 直达已落地，其余屏随后续阶段）；非 Ctrl 的字母永远不进命令层
    if mods.contains(KeyModifiers::CONTROL) {
        return match code {
            KeyCode::Char('q') | KeyCode::Char('c') => Some(Msg::Quit),
            // 屏幕直达（docs/03 §5.3 直达表落地：Practice=Ctrl+A、Review=Ctrl+R；其余屏随后续阶段）
            KeyCode::Char('a') => Some(Msg::Screen(Screen::Practice)),
            // M5-W7：翻译工作台直达（W 不在直达表保留字内，docs/03 §5.3）
            KeyCode::Char('w') => Some(Msg::Screen(Screen::Tx)),
            KeyCode::Char('r') => Some(Msg::Screen(Screen::Review)),
            KeyCode::Char('n') => Some(Msg::Screen(Screen::Notes)),
            KeyCode::Char('d') => Some(Msg::Screen(Screen::Diary)),
            KeyCode::Char('k') => Some(Msg::Screen(Screen::Todos)),
            KeyCode::Char('p') => Some(Msg::Screen(Screen::Profile)),
            KeyCode::Char('g') => Some(Msg::Screen(Screen::Games)),
            KeyCode::Char('v') => Some(Msg::Screen(Screen::Wordbook)),
            KeyCode::Char('s') if ctx == KeyCtx::Workspace => Some(Msg::WorkspaceSave),
            KeyCode::Char('s') if ctx == KeyCtx::Wordbook => Some(Msg::VocabEnroll),
            // M5-W6：开模考（上下文层语义——Ctrl+M 仅答题/完成态可开卷，docs/03 §5.3 直达表）
            KeyCode::Char('m')
                if matches!(ctx, KeyCtx::Choice | KeyCtx::Graded | KeyCtx::Finished) =>
            {
                Some(Msg::MockStart)
            }
            _ => None,
        };
    }
    // ② Esc / Enter：语义按上下文分化
    match code {
        KeyCode::Esc => {
            return match ctx {
                KeyCtx::Fill { editing: true } => Some(Msg::StopEdit),
                KeyCtx::Pending => Some(Msg::Next), // 跳过本题（保持待裁定桶）
                KeyCtx::Review => Some(Msg::Screen(Screen::Practice)), // 复习屏返回答题
                KeyCtx::Picker => Some(Msg::TogglePicker), // 面板返回答题
                KeyCtx::Scaffold => Some(Msg::Screen(Screen::Practice)), // 模板屏返回答题
                KeyCtx::Tx => Some(Msg::Screen(Screen::Practice)), // 工作台返回答题
                KeyCtx::TxAlign | KeyCtx::TxMyTurn | KeyCtx::TxHistory => {
                    Some(Msg::Screen(Screen::Tx)) // 子屏回主屏
                }
                KeyCtx::Workspace => Some(Msg::Screen(Screen::Practice)),
                KeyCtx::Profile => Some(Msg::Screen(Screen::Practice)),
                KeyCtx::Game => Some(Msg::Screen(Screen::Practice)),
                KeyCtx::Wordbook => Some(Msg::Screen(Screen::Practice)),
                _ => Some(Msg::Quit),
            };
        }
        KeyCode::Enter => {
            return Some(match ctx {
                KeyCtx::Choice | KeyCtx::Fill { .. } => Msg::Submit,
                KeyCtx::Pending => Msg::ConfirmArbitrate,
                KeyCtx::Graded | KeyCtx::Finished => Msg::Next,
                KeyCtx::Review => Msg::Flip, // 复习屏 Enter=翻面
                KeyCtx::Picker => Msg::Jump, // 面板 Enter=跳转到选中题
                KeyCtx::Scaffold => Msg::ScaffoldRender, // 模板屏 Enter=套槽成文
                KeyCtx::TxAlign => Msg::TxAlign, // 对齐视图 Enter=重对齐
                KeyCtx::Tx => Msg::TxTranslate, // 工作台 Enter=翻译
                KeyCtx::TxMyTurn => Msg::TxMyTurnSubmit, // 我来翻 Enter=对照评分
                KeyCtx::TxHistory => Msg::TxToPractice, // 历史 Enter=转练习题
                KeyCtx::Workspace => Msg::WorkspaceSave,
                KeyCtx::Profile => Msg::Screen(Screen::Practice),
                KeyCtx::Game => Msg::Screen(Screen::Practice),
                KeyCtx::Wordbook => Msg::VocabLookup,
            });
        }
        _ => {}
    }
    // ③ 填空待答：一切可打印字符（含空格/;/q/数字）都是文本——打字即恢复编辑；
    //    空位导航：Tab/→/↓ 下一空，Shift+Tab/←/↑ 上一空（单空时 update 原地）
    if matches!(ctx, KeyCtx::Fill { .. }) {
        return match code {
            KeyCode::Backspace => Some(Msg::Backspace),
            KeyCode::Char(c) => Some(Msg::Char(c)),
            KeyCode::Tab | KeyCode::Right | KeyCode::Down => Some(Msg::MoveDown),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Up => Some(Msg::MoveUp),
            _ => None,
        };
    }
    // ④ 作答 / 裁决 / 收尾（**仅非字母键：方向/Space/数字**；单字母快捷键全废，docs/03 §5.3 总则）
    match (ctx, code) {
        (KeyCtx::Choice, KeyCode::Down) => Some(Msg::MoveDown),
        (KeyCtx::Choice, KeyCode::Up) => Some(Msg::MoveUp),
        (KeyCtx::Choice, KeyCode::Char(' ')) => Some(Msg::Select),
        (KeyCtx::Choice, KeyCode::Char(c @ '1'..='4')) => {
            Some(Msg::Confidence(c.to_digit(10).unwrap_or(1) as u8))
        }
        (KeyCtx::Pending, KeyCode::Down | KeyCode::Right) => Some(Msg::MoveDown),
        (KeyCtx::Pending, KeyCode::Up | KeyCode::Left) => Some(Msg::MoveUp),
        // 复习屏（docs/03 §3 M4）：Space 翻面 · 1-4=Again/Hard/Good/Easy · Tab 返回
        (KeyCtx::Review, KeyCode::Char(' ')) => Some(Msg::Flip),
        (KeyCtx::Review, KeyCode::Char(c @ '1'..='4')) => {
            Some(Msg::Rate(c.to_digit(10).unwrap_or(1) as u8))
        }
        (KeyCtx::Review, KeyCode::Tab) => Some(Msg::Screen(Screen::Practice)),
        // 翻题：→ 跳过/下一题（不判分）、← 上一题（Choice/Graded，docs/03 §5.3 2026-09-27）
        (KeyCtx::Choice | KeyCtx::Graded, KeyCode::Right) => Some(Msg::Skip),
        (KeyCtx::Choice | KeyCtx::Graded, KeyCode::Left) => Some(Msg::Prev),
        // F1 选题面板（Choice/Graded/Finished 可呼出；面板内 F1 关闭）
        (KeyCtx::Choice | KeyCtx::Graded | KeyCtx::Finished, KeyCode::F(1)) => {
            Some(Msg::TogglePicker)
        }
        // F2 模板作文屏（Choice/Graded/Finished 可呼出，M5-W4b docs/03 §5.3）
        (KeyCtx::Choice | KeyCtx::Graded | KeyCtx::Finished, KeyCode::F(2)) => {
            Some(Msg::Screen(Screen::Scaffold))
        }
        (KeyCtx::Picker, KeyCode::F(1)) => Some(Msg::TogglePicker),
        // 面板内列表导航
        (KeyCtx::Picker, KeyCode::Down) => Some(Msg::MoveDown),
        (KeyCtx::Picker, KeyCode::Up) => Some(Msg::MoveUp),
        // —— 翻译工作台族（M5-W7，docs/11 §2；Enter/Esc 在②中心分化）——
        (KeyCtx::Tx, KeyCode::F(5)) => Some(Msg::TxAlign),
        (KeyCtx::Tx, KeyCode::F(6)) => Some(Msg::TxMyTurnOpen),
        (KeyCtx::Tx, KeyCode::F(7)) => Some(Msg::TxHistoryOpen),
        (KeyCtx::Tx, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::Tx, KeyCode::Backspace) => Some(Msg::Backspace),
        (KeyCtx::Workspace, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::Workspace, KeyCode::Backspace) => Some(Msg::Backspace),
        (KeyCtx::Game, KeyCode::F(n @ 1..=12)) => Some(Msg::GameAction(n)),
        (KeyCtx::Wordbook, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::Wordbook, KeyCode::Backspace) => Some(Msg::Backspace),
        (KeyCtx::Wordbook, KeyCode::F(3)) => Some(Msg::VocabEnroll),
        (KeyCtx::TxMyTurn, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::TxMyTurn, KeyCode::Backspace) => Some(Msg::Backspace),
        (KeyCtx::TxHistory, KeyCode::Up) => Some(Msg::TxHistoryUp),
        (KeyCtx::TxHistory, KeyCode::Down) => Some(Msg::TxHistoryDown),
        (KeyCtx::TxHistory, KeyCode::Char(' ')) => Some(Msg::TxToggleFavorite),
        // 判分后批注（M5-W5，F4——动作键走 F 区；Ctrl+E 保留 Writing 直达，docs/03 §5.3）
        (KeyCtx::Graded, KeyCode::F(4)) => Some(Msg::Annotate),
        // 模板作文屏（M5-W4b，docs/03 §5.3 scaffold 族）
        (KeyCtx::Scaffold, KeyCode::Up) => Some(Msg::ScaffoldUp),
        (KeyCtx::Scaffold, KeyCode::Down) => Some(Msg::ScaffoldDown),
        (KeyCtx::Scaffold, KeyCode::Tab | KeyCode::Right) => Some(Msg::ScaffoldSlotNext),
        (KeyCtx::Scaffold, KeyCode::BackTab | KeyCode::Left) => Some(Msg::ScaffoldSlotPrev),
        // M5-W4b：草稿自查（F3——动作键走 F 区；Ctrl+S 保留 Settings 直达，docs/03 §5.3）
        (KeyCtx::Scaffold, KeyCode::F(3)) => Some(Msg::ScaffoldCheck),
        (KeyCtx::Scaffold, KeyCode::Char(' ')) => Some(Msg::ScaffoldStage),
        (KeyCtx::Scaffold, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::Scaffold, KeyCode::Backspace) => Some(Msg::Backspace),
        _ => None,
    }
}

#[cfg(test)]
mod keys_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode as K, KeyModifiers as M};

    fn mods() -> M {
        M::empty()
    }

    fn all_ctxs() -> [KeyCtx; 9] {
        [
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
        // 返回答题：Tab / Esc
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Tab, mods()),
            Some(Screen(crate::Screen::Practice))
        );
        assert_eq!(
            key_to_msg(KeyCtx::Review, K::Esc, mods()),
            Some(Screen(crate::Screen::Practice)),
            "Esc 返回而非退出"
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
        assert!(hint_review.contains("翻面"), "{hint_review}");
        assert!(hint_review.contains("1-4"), "{hint_review}");
        for ctx in [KeyCtx::Choice, KeyCtx::Graded, KeyCtx::Finished] {
            assert_ne!(hint(ctx), hint_review, "提示互异 {ctx:?}");
        }
    }
}
