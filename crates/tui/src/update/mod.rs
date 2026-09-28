//! 状态转移（细分域根）：update（选择勾选/多选/填空编辑/提交/裁决/自评/翻题/选题面板，纯函数副作用回 Cmd）
//! 本域含配对测试子组件：`update::update_tests`（状态机）/ `update::update_passage_tests`（语篇作答）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

#[cfg(test)]
mod update_passage_tests;
#[cfg(test)]
mod update_tests;

use crate::*;

/// 纯状态机：键位语义见 keys.rs（唯一键位表），本层只做状态转移与防误触双保险
pub fn update(state: &mut AppState, msg: Msg) -> Cmd {
    // 屏幕切换（任何状态可用：Ctrl+A/Ctrl+R 直达、Tab 进出复习屏；进 Review 触发队列加载）
    if let Msg::Screen(target) = msg {
        if state.screen == target {
            return Cmd::None;
        }
        state.screen = target;
        return if target == Screen::Review {
            Cmd::ReviewLoad
        } else {
            Cmd::None
        };
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
    // 选题面板独立状态机（↑↓ 列表 · Enter 跳转复位 · 其余消息忽略；Esc/F1 经 TogglePicker 已拦截）
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
    if state.finished {
        return if matches!(msg, Msg::Quit) {
            Cmd::Quit
        } else {
            Cmd::None
        };
    }
    match msg {
        Msg::Quit => Cmd::Quit,
        Msg::Char(c) => {
            // 打字即编辑（无需进入键）：首键进入编辑态，字符落当前空位（keys.rs Fill ctx 路由）
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
            // 退出编辑但保留已输入（再打字直接恢复；非编辑态忽略）
            if state.is_fill_pending() && state.editing {
                state.editing = false;
            }
            Cmd::None
        }
        Msg::Select => {
            // Space 勾选/取消（2026-09-27 修订：单选也可勾选 = 选择状态可见）；仅作答态有效
            if state.grade.is_some() {
                return Cmd::None;
            }
            match &state.questions[state.idx].kind {
                // 单选：单元素勾选替换（同位再按=取消；✓ 可见，提交勾选项优先）
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
                // 多选：切换选中集（M1 既有）
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
                // 七选五/阅读：逐槽作答——记录当前槽选中项并前进到下一槽（M4 题型补全）
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
            // 仅待裁定态生效：确认当前高亮裁决（0=判对 1=判错）；其余状态忽略（防误触）
            if state
                .grade
                .as_ref()
                .is_some_and(|g| g.verdict == ambition_core::Verdict::Pending)
            {
                return Cmd::Arbitrate(state.arbitration_sel == 0);
            }
            Cmd::None
        }
        Msg::Confidence(level) => {
            if state.grade.is_none() && !state.is_fill_pending() {
                state.self_confidence = Some(level.clamp(1, 4));
            }
            Cmd::None
        }
        Msg::MoveDown | Msg::MoveUp => {
            // 待裁定：箭头切换「判对/判错」高亮（不计修订）
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
            // 填空待答：切换空位（Tab/→/↓ 下一空，Shift+Tab/←/↑ 上一空；单空原地不动）
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
            // 阅读：光标在「当前子题」的选项间移动（逐槽作答，M4 题型补全）
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
        Msg::Submit => {
            if state.grade.is_some() {
                return Cmd::None;
            }
            state.error = None; // 重试前清掉上一次的错误行
            match &state.questions[state.idx].kind {
                QuestionKind::SingleChoice(_) => {
                    // 勾选项优先（Space 勾了再移动光标不丢）；未勾 = 按光标位提交
                    let pick = state.checked.first().copied().unwrap_or(state.sel as u32);
                    Cmd::Submit(Answer::Choice(vec![pick]))
                }
                QuestionKind::MultiChoice(_) => {
                    // 空选提交报错不提交（2026-09-27 修订：不再静默回退光标位）
                    if state.checked.is_empty() {
                        state.error = Some("请先用 Space 选中选项再提交（→ 跳过本题）".to_string());
                        return Cmd::None;
                    }
                    Cmd::Submit(Answer::Choice(state.checked.clone()))
                }
                QuestionKind::FillBlank(body) => {
                    // 每空一位（长度由构造保证；不齐 = 状态异常，防御性报错不伪造提交）
                    if state.inputs.len() != body.blanks.len() {
                        state.error = Some(format!(
                            "本题 {} 个空，当前 {} 个输入位（状态异常）",
                            body.blanks.len(),
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
                // M4 题型补全：完形/语法填空 = 逐空填空（与 FillBlank 同路，子题递归判分）
                QuestionKind::Cloze(cloze) => {
                    if state.inputs.len() != cloze.blanks.len() {
                        state.error = Some(format!(
                            "本题 {} 个空，当前 {} 个输入位（状态异常）",
                            cloze.blanks.len(),
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
                QuestionKind::GrammarFill(gf) => {
                    if state.inputs.len() != gf.blanks.len() {
                        state.error = Some(format!(
                            "本题 {} 个空，当前 {} 个输入位（状态异常）",
                            gf.blanks.len(),
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
                // 七选五：逐缺口（sub_answers 逐槽记录；未记录槽回退光标位）
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
                // 阅读：逐子题 → Answer::Group（未记录子题回退光标位并钳制到选项范围）
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
            }
        }
        Msg::Next => {
            if state.grade.is_none() {
                return Cmd::None;
            }
            advance_question(state);
            Cmd::None
        }
        // → 跳过/下一题（不判分，docs/03 §5.3 2026-09-27）
        Msg::Skip => {
            advance_question(state);
            Cmd::None
        }
        // ← 上一题（首题原地；状态复位，已作答不保留——docs/03 §5.3 2026-09-27）
        Msg::Prev => {
            if state.idx > 0 {
                state.idx -= 1;
                reset_question_state(state);
            }
            Cmd::None
        }
        // 复习屏/面板专属消息在答题流程忽略（顶部分支已拦截；防误触）
        Msg::Screen(_) | Msg::Flip | Msg::Rate(_) | Msg::TogglePicker | Msg::Jump => Cmd::None,
    }
}

/// 复位当前题作答状态（Next/Skip/Prev/Jump 共用；含错误行清理）
fn reset_question_state(state: &mut AppState) {
    state.grade = None;
    state.sel = 0;
    state.revisions = 0;
    state.self_confidence = None;
    state.editing = false;
    state.checked.clear();
    state.sub_answers.clear();
    state.srs_hint = None;
    state.arbitration_sel = 0;
    state.error = None;
    state.started_at = Instant::now();
    // 按当前题重建空位输入（选择题 = 0 位）
    state.inputs = vec![String::new(); blank_count(&state.questions[state.idx].kind)];
    state.blank_sel = 0;
}

/// 前进一题并复位（Next/Skip 共用）；越界 = 完成屏
fn advance_question(state: &mut AppState) {
    if state.idx + 1 >= state.questions.len() {
        state.finished = true;
        reset_question_state(state);
        return;
    }
    state.idx += 1;
    reset_question_state(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::Verdict;

    // 配对桩（同文件强制配对；完整场景测试在 update_tests.rs）

    #[test]
    fn test_update_moves_cursor_and_submits_choice() {
        let mut s = new_state(vec![crate::sample_question()]);
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
        assert_eq!(s.sel, 1, "光标下移");
        let cmd = update(&mut s, Msg::Submit);
        assert!(matches!(cmd, Cmd::Submit(Answer::Choice(_))), "提交选择");
        // 判分后 Submit 被忽略
        s.grade = Some(GradeResult {
            verdict: Verdict::Correct,
            score: 1.0,
            max: 1.0,
            confidence: 1.0,
            graded_by: ambition_core::GradedBy::Rule,
            items: vec![],
            error_tags: vec![],
        });
        assert_eq!(update(&mut s, Msg::Submit), Cmd::None, "已判分忽略提交");
    }

    #[test]
    fn test_reset_question_state_clears_every_answer_field() {
        let mut s = new_state(vec![crate::sample_question()]);
        s.sel = 1;
        s.revisions = 5;
        s.self_confidence = Some(2);
        s.editing = true;
        s.checked = vec![1];
        s.sub_answers = vec![0];
        s.srs_hint = Some("x".into());
        s.arbitration_sel = 1;
        s.error = Some("boom".into());
        reset_question_state(&mut s);
        assert_eq!(
            (
                s.grade.is_none(),
                s.sel,
                s.revisions,
                s.self_confidence,
                s.editing,
                s.checked.is_empty(),
                s.sub_answers.is_empty(),
                s.srs_hint.is_none(),
                s.arbitration_sel,
                s.error.is_none(),
            ),
            (true, 0, 0, None, false, true, true, true, 0, true),
            "全部复位"
        );
    }

    #[test]
    fn test_advance_question_reaches_finished_at_tail() {
        let mut s = new_state(vec![crate::sample_question()]);
        advance_question(&mut s);
        assert!(s.finished, "末题前进 = 完成屏");
        assert_eq!(s.idx, 0, "完成屏不越界");
    }
}
