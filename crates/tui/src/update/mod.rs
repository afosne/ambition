//! 状态转移（细分域根）：update（选择勾选/多选/填空编辑/提交/裁决/自评/翻题/选题面板，纯函数副作用回 Cmd）
//! 本域含配对测试子组件：`update::update_tests`（状态机）/ `update::update_passage_tests`（语篇作答）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

mod home_update;
mod m6;
mod mock;
mod nav;
mod practice;
mod scaffold;
mod tx;
#[cfg(test)]
mod update_mod_tests;
#[cfg(test)]
mod update_passage_tests;
#[cfg(test)]
mod update_tests;

use self::home_update::{home_enter, home_menu_len};
use self::nav::reset_question_state;
use crate::*;

pub use mock::{apply_mock_finish, apply_mock_start};
pub use scaffold::{apply_scaffold_check, apply_scaffold_result, apply_scaffold_templates};
pub use tx::{
    apply_tx_align, apply_tx_compare, apply_tx_history, apply_tx_practice, apply_tx_result, tx_opts,
};

pub use home_update::{
    FOOTER_LINES, HEADER_LINES, LINES_PER_ITEM, home_ensure_cursor_visible, home_max_scroll,
};

/// 纯状态机：键位语义见 keys.rs（唯一键位表），本层只做状态转移与防误触双保险
pub fn update(state: &mut AppState, msg: Msg) -> Cmd {
    // 屏幕切换（任何状态可用：Ctrl+A/Ctrl+R 直达、Tab 进出复习屏；进 Review 触发队列加载）
    if let Msg::Screen(target) = msg {
        return m6::update_screen(state, target);
    }
    // 主页面状态机（↑↓ 导航 · Enter 进入功能屏 · Esc 退出已在 keys 路由）
    if state.screen == Screen::Home {
        return match msg {
            Msg::Quit => Cmd::Quit,
            Msg::MoveDown => {
                let n = home_menu_len();
                state.home_cursor = (state.home_cursor + 1) % n;
                home_ensure_cursor_visible(state);
                Cmd::None
            }
            Msg::MoveUp => {
                let n = home_menu_len();
                state.home_cursor = (state.home_cursor + n - 1) % n;
                home_ensure_cursor_visible(state);
                Cmd::None
            }
            Msg::HomeScrollUp => {
                state.home_scroll = state.home_scroll.saturating_sub(LINES_PER_ITEM);
                Cmd::None
            }
            Msg::HomeScrollDown => {
                // 优先使用渲染层回填的实际 max_scroll；首帧前回填值=0 → 兌底到保守估计
                let max = if state.home_max_scroll_actual > 0 {
                    state.home_max_scroll_actual
                } else {
                    home_max_scroll()
                };
                state.home_scroll = (state.home_scroll + LINES_PER_ITEM).min(max);
                Cmd::None
            }
            Msg::HomeClick(i) => {
                let n = home_menu_len();
                state.home_cursor = i.min(n - 1);
                home_enter(state)
            }
            Msg::HoverHome(i) => {
                let n = home_menu_len();
                state.home_cursor = i.min(n - 1);
                home_ensure_cursor_visible(state);
                Cmd::None
            }
            Msg::HomeEnter => home_enter(state),
            // #18 题包导入/导出（Ctrl+O / Ctrl+X 在 Home 屏路由 → main 副作用）
            Msg::PackImport => Cmd::PackImport(String::new()),
            Msg::PackExport => Cmd::PackExport,
            _ => Cmd::None,
        };
    }
    if let Some(cmd) = m6::update_m6(state, msg) {
        return cmd;
    }
    // F1 选题面板开/关（任何有题状态可呼出，含完成屏；docs/03 §5.3 2026-09-27）
    if matches!(msg, Msg::TogglePicker) {
        state.picker_open = !state.picker_open;
        if state.picker_open {
            state.picker_cursor = state.idx.min(state.questions.len().saturating_sub(1));
        }
        return Cmd::None;
    }
    // 复习屏独立状态机（docs/03 §3 M4：翻面 → 1-4 评分；未翻面评分忽略防误触）
    if state.screen == Screen::Review {
        return match msg {
            Msg::Quit => Cmd::Quit,
            Msg::Flip => {
                state.review.revealed = true;
                Cmd::None
            }
            Msg::Rate(level) => {
                if state.review.revealed && state.review.cards.get(state.review.idx).is_some() {
                    Cmd::ReviewRate(level)
                } else {
                    Cmd::None
                }
            }
            _ => Cmd::None,
        };
    }
    // 选题面板独立状态机
    if state.picker_open {
        return match msg {
            Msg::Quit => Cmd::Quit,
            Msg::MoveDown => {
                if !state.questions.is_empty() {
                    state.picker_cursor = (state.picker_cursor + 1) % state.questions.len();
                }
                Cmd::None
            }
            Msg::MoveUp => {
                if !state.questions.is_empty() {
                    state.picker_cursor =
                        (state.picker_cursor + state.questions.len() - 1) % state.questions.len();
                }
                Cmd::None
            }
            Msg::Jump => {
                if state.questions.get(state.picker_cursor).is_some() {
                    state.idx = state.picker_cursor;
                    state.finished = false;
                    reset_question_state(state);
                    state.picker_open = false;
                }
                Cmd::None
            }
            _ => Cmd::None,
        };
    }
    // 模板作文屏独立状态机
    if state.screen == Screen::Scaffold {
        return scaffold::scaffold_update(state, msg);
    }
    // 翻译工作台族独立状态机
    if matches!(
        state.screen,
        Screen::Tx | Screen::TxAlign | Screen::TxMyTurn | Screen::TxHistory
    ) {
        return tx::tx_update(state, msg);
    }
    if state.finished {
        if matches!(msg, Msg::MockStart) {
            // 收卷完成态仍可开新卷（Ctrl+M；M5-W6）——落到主状态机
        } else {
            return if matches!(msg, Msg::Quit) {
                Cmd::Quit
            } else {
                Cmd::None
            };
        }
    }
    // 练习屏主消息分发（打字/选择/裁决/提交/翻题/鼠标交互）
    practice::practice_msg(state, msg)
}

/// 提交答案（按题型构造 Answer）——从 update 主线拆出以控制行数
pub(crate) fn submit_answer(state: &mut AppState) -> Cmd {
    if state.grade.is_some() {
        return Cmd::None;
    }
    state.error = None;
    match &state.questions[state.idx].kind {
        QuestionKind::SingleChoice(_) => {
            let pick = state.checked.first().copied().unwrap_or(state.sel as u32);
            Cmd::Submit(Answer::Choice(vec![pick]))
        }
        QuestionKind::MultiChoice(_) => {
            if state.checked.is_empty() {
                state.error = Some("请先用 Space 选中选项再提交（→ 跳过本题）".to_string());
                return Cmd::None;
            }
            Cmd::Submit(Answer::Choice(state.checked.clone()))
        }
        QuestionKind::FillBlank(body) => {
            let blanks = body.blanks.clone();
            fill_submit(state, &blanks)
        }
        QuestionKind::Cloze(body) => {
            let blanks = body.blanks.clone();
            fill_submit(state, &blanks)
        }
        QuestionKind::GrammarFill(body) => {
            let blanks = body.blanks.clone();
            fill_submit(state, &blanks)
        }
        QuestionKind::SevenOfFive(body) => {
            let gaps = body.answer.len();
            let mut ids = Vec::with_capacity(gaps);
            for gap in 0..gaps {
                ids.push(
                    state
                        .sub_answers
                        .get(gap)
                        .copied()
                        .unwrap_or(state.sel as u32),
                );
            }
            Cmd::Submit(Answer::Choice(ids))
        }
        QuestionKind::Reading(body) => {
            let mut subs = Vec::with_capacity(body.items.len());
            for (i, item) in body.items.iter().enumerate() {
                let pick = state
                    .sub_answers
                    .get(i)
                    .copied()
                    .unwrap_or(state.sel as u32)
                    .min(item.options.len().saturating_sub(1) as u32);
                subs.push(Answer::Choice(vec![pick]));
            }
            Cmd::Submit(Answer::Group(subs))
        }
        QuestionKind::Translation(_) | QuestionKind::Essay(_) => {
            let hint = if matches!(
                state.questions[state.idx].kind,
                QuestionKind::Translation(_)
            ) {
                "译文"
            } else {
                "作文"
            };
            let text = state
                .inputs
                .first()
                .map(|field| field.trim().to_string())
                .unwrap_or_default();
            if text.is_empty() {
                state.error = Some(format!("请先输入{hint}内容再提交（→ 跳过本题）"));
                return Cmd::None;
            }
            Cmd::Submit(Answer::Text(text))
        }
    }
}

/// 填空类提交辅助（FillBlank/Cloze/GrammarFill 共用，从 submit_answer 拆出以控制行数）
pub(crate) fn fill_submit(state: &mut AppState, blanks: &[ambition_core::BlankItem]) -> Cmd {
    if state.inputs.len() != blanks.len() {
        state.error = Some(format!(
            "本题 {} 个空，当前 {} 个输入位（状态异常）",
            blanks.len(),
            state.inputs.len()
        ));
        return Cmd::None;
    }
    Cmd::Submit(Answer::Fill(
        state
            .inputs
            .iter()
            .map(|field| field.trim().to_string())
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 配对桩：fn update → test_update（同文件配对门禁）
    // 完整场景测试在 update_mod_tests.rs
    #[test]
    fn test_update_quit_returns_quit_cmd() {
        let mut s = new_state(vec![crate::sample_question()]);
        assert_eq!(update(&mut s, Msg::Quit), Cmd::Quit);
    }

    // 配对桩：fn submit_answer → test_submit_answer（同文件配对门禁）
    #[test]
    fn test_submit_answer_returns_choice_for_single() {
        let mut s = new_state(vec![crate::sample_question()]);
        let cmd = submit_answer(&mut s);
        assert!(matches!(cmd, Cmd::Submit(Answer::Choice(_))));
    }

    // 配对桩：fn fill_submit → test_fill_submit（同文件配对门禁）
    #[test]
    fn test_fill_submit_rejects_mismatched_blank_count() {
        use ambition_core::{BlankItem, FillBody, QuestionKind};
        let q = Question::new(
            "test".into(),
            QuestionKind::FillBlank(FillBody {
                sentence: "___".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["x".into()],
                }],
            }),
            1,
            vec![],
            ambition_core::Provenance::Seed,
        );
        let mut s = new_state(vec![q]);
        s.inputs.clear(); // 清空造成不齐（1 blank vs 0 inputs）
        let blanks = vec![BlankItem {
            acceptable: vec!["x".into()],
        }];
        let cmd = fill_submit(&mut s, &blanks);
        assert_eq!(cmd, Cmd::None);
        assert!(s.error.is_some(), "不齐报错");
    }
}
