//! 练习屏消息处理（从 mod.rs 拆出以控制行数 ≤500）
use crate::*;

/// 练习屏主消息分发（打字/选择/裁决/提交/翻题/鼠标交互）
pub fn practice_msg(state: &mut AppState, msg: Msg) -> Cmd {
    match msg {
        Msg::Quit => Cmd::Quit,
        Msg::Char(c) => {
            // 打字即编辑（无需进入键）：首键进入编辑态，字符落当前空位
            if state.is_fill_pending() {
                state.editing = true;
                if let Some(field) = state.inputs.get_mut(state.blank_sel) {
                    field.push(c);
                    state.revisions += 1;
                }
            }
            Cmd::None
        }
        Msg::Backspace => {
            if state.is_fill_pending() {
                state.editing = true;
                if state
                    .inputs
                    .get_mut(state.blank_sel)
                    .is_some_and(|field| field.pop().is_some())
                {
                    state.revisions += 1;
                }
            }
            Cmd::None
        }
        Msg::StopEdit => {
            if state.is_fill_pending() && state.editing {
                state.editing = false;
            }
            Cmd::None
        }
        Msg::ClickOption(i) => {
            // 鼠标点击选项：设 sel=i 后走 Select 逻辑
            let opts_len = match &state.questions[state.idx].kind {
                QuestionKind::SingleChoice(b) | QuestionKind::MultiChoice(b) => b.options.len(),
                _ => 0,
            };
            if opts_len == 0 || i >= opts_len || state.grade.is_some() {
                return Cmd::None;
            }
            state.sel = i;
            // fallthrough 到 Select 逻辑
            match &state.questions[state.idx].kind {
                QuestionKind::SingleChoice(_) => {
                    let id = state.sel as u32;
                    if state.checked.contains(&id) {
                        state.checked.clear();
                    } else {
                        state.checked = vec![id];
                    }
                    state.revisions += 1;
                    Cmd::None
                }
                QuestionKind::MultiChoice(_) => {
                    let id = state.sel as u32;
                    if let Some(pos) = state.checked.iter().position(|&x| x == id) {
                        state.checked.remove(pos);
                    } else {
                        state.checked.push(id);
                        state.checked.sort_unstable();
                    }
                    state.revisions += 1;
                    Cmd::None
                }
                _ => Cmd::None,
            }
        }
        Msg::ClickBlank(i) => {
            if state.grade.is_some() || i >= state.inputs.len() {
                return Cmd::None;
            }
            state.blank_sel = i;
            state.editing = true;
            Cmd::None
        }
        Msg::HoverOption(i) => {
            let opts_len = match &state.questions[state.idx].kind {
                QuestionKind::SingleChoice(b) | QuestionKind::MultiChoice(b) => b.options.len(),
                _ => 0,
            };
            if opts_len == 0 || i >= opts_len || state.grade.is_some() {
                return Cmd::None;
            }
            state.sel = i;
            Cmd::None
        }
        Msg::HoverBlank(i) => {
            if state.grade.is_some() || i >= state.inputs.len() {
                return Cmd::None;
            }
            state.blank_sel = i;
            Cmd::None
        }
        Msg::Select => {
            if state.grade.is_some() {
                return Cmd::None;
            }
            match &state.questions[state.idx].kind {
                QuestionKind::SingleChoice(_) => {
                    let id = state.sel as u32;
                    if state.checked.contains(&id) {
                        state.checked.clear();
                    } else {
                        state.checked = vec![id];
                    }
                    state.revisions += 1;
                    Cmd::None
                }
                QuestionKind::MultiChoice(_) => {
                    let id = state.sel as u32;
                    if let Some(pos) = state.checked.iter().position(|&x| x == id) {
                        state.checked.remove(pos);
                    } else {
                        state.checked.push(id);
                        state.checked.sort_unstable();
                    }
                    state.revisions += 1;
                    Cmd::None
                }
                QuestionKind::SevenOfFive(body) if state.sub_answers.len() < body.answer.len() => {
                    state.sub_answers.push(state.sel as u32);
                    state.sel = 0;
                    state.revisions += 1;
                    Cmd::None
                }
                QuestionKind::Reading(body) if state.sub_answers.len() < body.items.len() => {
                    state.sub_answers.push(state.sel as u32);
                    state.sel = 0;
                    state.revisions += 1;
                    Cmd::None
                }
                _ => Cmd::None,
            }
        }
        Msg::ConfirmArbitrate => {
            if state
                .grade
                .as_ref()
                .is_some_and(|g| g.verdict == ambition_core::Verdict::Pending)
            {
                return Cmd::Arbitrate(state.arbitration_sel == 0);
            }
            Cmd::None
        }
        Msg::MoveDown | Msg::MoveUp => {
            if state
                .grade
                .as_ref()
                .is_some_and(|g| g.verdict == ambition_core::Verdict::Pending)
            {
                state.arbitration_sel = 1 - state.arbitration_sel;
                return Cmd::None;
            }
            if state.grade.is_some() {
                return Cmd::None;
            }
            if state.is_fill_pending() {
                let n = state.inputs.len();
                if n > 1 {
                    let delta = if matches!(msg, Msg::MoveDown) {
                        1
                    } else {
                        n - 1
                    };
                    state.blank_sel = (state.blank_sel + delta) % n;
                    state.revisions += 1;
                }
                return Cmd::None;
            }
            let kind = &state.questions[state.idx].kind;
            let option_count = match kind {
                QuestionKind::Reading(body) => body
                    .items
                    .get(state.sub_answers.len())
                    .map(|item| item.options.len())
                    .unwrap_or(0),
                other => other.option_count(),
            };
            if option_count == 0 {
                return Cmd::None;
            }
            let delta = if matches!(msg, Msg::MoveDown) {
                1
            } else {
                option_count - 1
            };
            state.sel = (state.sel + delta) % option_count;
            state.revisions += 1;
            Cmd::None
        }
        Msg::Submit => super::submit_answer(state),
        Msg::Next => {
            if state.grade.is_none() {
                return Cmd::None;
            }
            crate::update::nav::advance_question(state);
            if state.finished && state.mock.is_some() {
                return Cmd::MockFinish;
            }
            Cmd::None
        }
        Msg::Skip => {
            crate::update::nav::advance_question(state);
            if state.finished && state.mock.is_some() {
                return Cmd::MockFinish;
            }
            Cmd::None
        }
        Msg::Prev => {
            let is_composite = matches!(
                state.questions.get(state.idx).map(|q| &q.kind),
                Some(QuestionKind::SevenOfFive(_) | QuestionKind::Reading(_))
            );
            if is_composite && !state.sub_answers.is_empty() {
                state.sub_answers.pop();
                state.sel = 0;
                state.revisions += 1;
                return Cmd::None;
            }
            if state.idx > 0 {
                state.idx -= 1;
                crate::update::nav::reset_question_state(state);
            }
            Cmd::None
        }
        Msg::MockStart => {
            if state.screen == Screen::Mock && !state.finished {
                return Cmd::None;
            }
            Cmd::MockStart
        }
        Msg::Annotate => {
            let text_kind = matches!(
                state.questions[state.idx].kind,
                QuestionKind::Essay(_) | QuestionKind::Translation(_)
            );
            if state.grade.is_none() || !text_kind {
                return Cmd::None;
            }
            match state.submitted_text.clone() {
                Some(text) if !text.trim().is_empty() => Cmd::Annotate(text),
                _ => {
                    state.error = Some("无可批注文本（先作答提交）".to_string());
                    Cmd::None
                }
            }
        }
        Msg::TxTranslate
        | Msg::TxAlign
        | Msg::TxMyTurnOpen
        | Msg::TxMyTurnSubmit
        | Msg::TxHistoryOpen
        | Msg::TxHistoryUp
        | Msg::TxHistoryDown
        | Msg::TxToggleFavorite
        | Msg::TxToPractice => Cmd::None,
        Msg::WorkspaceSave => Cmd::None,
        Msg::GameAction(_) => Cmd::None,
        Msg::VocabLookup | Msg::VocabEnroll => Cmd::None,
        Msg::ScaffoldUp
        | Msg::ScaffoldDown
        | Msg::ScaffoldStage
        | Msg::ScaffoldSlotNext
        | Msg::ScaffoldSlotPrev
        | Msg::ScaffoldRender
        | Msg::ScaffoldCheck => Cmd::None,
        Msg::Screen(_)
        | Msg::Flip
        | Msg::Rate(_)
        | Msg::TogglePicker
        | Msg::ToggleHelp
        | Msg::ToggleExplain
        | Msg::Jump
        | Msg::HomeEnter
        | Msg::HomeClick(_)
        | Msg::HoverHome(_)
        | Msg::HomeScrollUp
        | Msg::HomeScrollDown
        | Msg::PackImport
        | Msg::PackExport
        | Msg::PracticeSelect => Cmd::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_practice_msg_moves_cursor_and_submits() {
        let mut s = new_state(vec![crate::sample_question()]);
        assert_eq!(practice_msg(&mut s, Msg::MoveDown), Cmd::None);
        assert_eq!(s.sel, 1);
        assert!(matches!(
            practice_msg(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Choice(_))
        ));
    }
}
