//! 状态转移：update（选择/多选选中/填空编辑/提交/裁决/自评，纯函数副作用回 Cmd）

use super::*;

/// 纯状态机：键位语义见 keys.rs（唯一键位表），本层只做状态转移与防误触双保险
pub fn update(state: &mut AppState, msg: Msg) -> Cmd {
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
            // Space 选中/取消：多选=切换选中集；单选无意义（光标即答案）；仅作答态有效
            if state.grade.is_some() {
                return Cmd::None;
            }
            if matches!(
                &state.questions[state.idx].kind,
                QuestionKind::MultiChoice(_)
            ) {
                let id = state.sel as u32;
                if let Some(pos) = state.checked.iter().position(|&x| x == id) {
                    state.checked.remove(pos);
                } else {
                    state.checked.push(id);
                    state.checked.sort_unstable();
                }
                state.revisions += 1;
            }
            Cmd::None
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
            let option_count = kind.option_count();
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
                    Cmd::Submit(Answer::Choice(vec![state.sel as u32]))
                }
                QuestionKind::MultiChoice(_) => {
                    // 选中集优先（Space 选了再移动光标不丢）；未选中 = 按光标位提交
                    let ids = if state.checked.is_empty() {
                        vec![state.sel as u32]
                    } else {
                        state.checked.clone()
                    };
                    Cmd::Submit(Answer::Choice(ids))
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
                // 七选五：逐缺口选中（checked = 各缺口选中项；未选中回退光标位）
                QuestionKind::SevenOfFive(body) => {
                    let gaps = body.answer.len();
                    let mut ids = Vec::with_capacity(gaps);
                    for gap in 0..gaps {
                        ids.push(state.checked.get(gap).copied().unwrap_or(state.sel as u32));
                    }
                    Cmd::Submit(Answer::Choice(ids))
                }
                // 阅读：逐子题选中（checked = 各子题选中项；未选中回退光标位）
                QuestionKind::Reading(body) => {
                    let mut subs = Vec::with_capacity(body.items.len());
                    for (i, item) in body.items.iter().enumerate() {
                        let pick = state
                            .checked
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
            state.grade = None;
            state.sel = 0;
            state.revisions = 0;
            state.self_confidence = None;
            state.editing = false;
            state.checked.clear();
            state.arbitration_sel = 0;
            state.started_at = Instant::now();
            if state.idx + 1 >= state.questions.len() {
                state.finished = true;
            } else {
                state.idx += 1;
                // 按新题重建空位输入（选择题 = 0 位）
                state.inputs = vec![String::new(); blank_count(&state.questions[state.idx].kind)];
                state.blank_sel = 0;
            }
            Cmd::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::Verdict;

    // 配对桩（门禁：同文件 fn↔test 配对；完整场景测试在 update_tests.rs）

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
}
