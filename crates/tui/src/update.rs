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
            // 打字即编辑（无需进入键）：首键进入编辑态（keys.rs Fill ctx 路由）
            if state.is_fill_pending() {
                state.editing = true;
                state.input.push(c);
                state.revisions += 1;
            }
            Cmd::None
        }
        Msg::Backspace => {
            if state.is_fill_pending() {
                state.editing = true;
                if state.input.pop().is_some() {
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
        Msg::Arbitrate(accept) => {
            // 仅待裁定态响应裁决键；其余状态忽略（防误触）
            if state
                .grade
                .as_ref()
                .is_some_and(|g| g.verdict == ambition_core::Verdict::Pending)
            {
                return Cmd::Arbitrate(accept);
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
            if state.grade.is_some() {
                return Cmd::None;
            }
            let kind = &state.questions[state.idx].kind;
            let option_count = match kind {
                QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) => {
                    body.options.len()
                }
                QuestionKind::FillBlank(_) => 0,
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
                    // 多空输入：`;` 分隔（单空直接整串）
                    let answers: Vec<String> = state
                        .input
                        .split(';')
                        .map(|part| part.trim().to_string())
                        .collect();
                    if answers.len() != body.blanks.len() {
                        state.error = Some(format!(
                            "本题 {} 个空，请用 ; 分隔 {} 个答案",
                            body.blanks.len(),
                            body.blanks.len()
                        ));
                        return Cmd::None;
                    }
                    Cmd::Submit(Answer::Fill(
                        state
                            .input
                            .split(';')
                            .map(|p| p.trim().to_string())
                            .collect(),
                    ))
                }
            }
        }
        Msg::Next => {
            if state.grade.is_none() {
                return Cmd::None;
            }
            state.grade = None;
            state.sel = 0;
            state.input.clear();
            state.revisions = 0;
            state.self_confidence = None;
            state.editing = false;
            state.checked.clear();
            state.started_at = Instant::now();
            if state.idx + 1 >= state.questions.len() {
                state.finished = true;
            } else {
                state.idx += 1;
            }
            Cmd::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{BlankItem, FillBody, GradedBy, Provenance, Verdict};

    fn grade(verdict: Verdict) -> GradeResult {
        GradeResult {
            verdict,
            score: if verdict == Verdict::Correct {
                1.0
            } else {
                0.0
            },
            max: 1.0,
            confidence: 1.0,
            graded_by: GradedBy::Rule,
            items: vec![],
            error_tags: vec![],
        }
    }

    fn fill_question(blanks: usize) -> Question {
        let sentence = if blanks == 1 {
            "I ___ a student.".to_string()
        } else {
            "She ___ to school ___ bus.".to_string()
        };
        Question::new(
            sentence.clone(),
            QuestionKind::FillBlank(FillBody {
                sentence,
                blanks: (0..blanks)
                    .map(|_| BlankItem {
                        acceptable: vec!["am".into(), "by".into()],
                    })
                    .collect(),
            }),
            1,
            vec!["grammar".into()],
            Provenance::Seed,
        )
    }

    /// 多选题夹具（测试内 helper）
    fn multi_question() -> Question {
        Question::new(
            "Pick two.".into(),
            QuestionKind::MultiChoice(ambition_core::ChoiceBody {
                options: (0..3)
                    .map(|i| ambition_core::OptionItem {
                        id: i,
                        text: format!("opt{i}"),
                    })
                    .collect(),
                answer: vec![0, 2],
            }),
            2,
            vec![],
            Provenance::Seed,
        )
    }

    #[test]
    fn test_update_moves_selection_with_wrap() {
        let mut s = new_state(vec![sample_question()]);
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
        assert_eq!(s.sel, 1);
        assert_eq!(update(&mut s, Msg::MoveUp), Cmd::None);
        assert_eq!(s.sel, 0);
        assert_eq!(update(&mut s, Msg::MoveUp), Cmd::None, "向上环绕到末项");
        assert_eq!(s.sel, 3);
        update(&mut s, Msg::MoveDown);
        assert_eq!(s.sel, 0, "向下环绕回首项");
        assert!(s.revisions >= 3, "移动计入修订次数");
    }

    #[test]
    fn test_update_submits_answer_and_ignores_after_grade() {
        let mut s = new_state(vec![sample_question()]);
        update(&mut s, Msg::MoveDown);
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![1]))
        );
        apply_grade(&mut s, grade(Verdict::Correct));
        // 已判分：提交/移动无效
        assert_eq!(update(&mut s, Msg::Submit), Cmd::None);
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
        assert_eq!(s.sel, 1, "判分后选择不移动");
    }

    #[test]
    fn test_update_fill_input_builds_answer_and_counts_revisions() {
        let mut s = new_state(vec![fill_question(1)]);
        assert_eq!(update(&mut s, Msg::Char('a')), Cmd::None);
        assert_eq!(update(&mut s, Msg::Char('m')), Cmd::None);
        assert_eq!(s.input, "am");
        assert_eq!(s.revisions, 2, "逐字符计入修订");
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["am".to_string()])),
            "单空整串提交"
        );
        assert_eq!(update(&mut s, Msg::Backspace), Cmd::None, "判分后不删改");

        // 多空分号分隔
        let mut multi = new_state(vec![fill_question(2)]);
        for c in "am;by".chars() {
            update(&mut multi, Msg::Char(c));
        }
        assert_eq!(
            update(&mut multi, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["am".to_string(), "by".to_string()]))
        );
    }

    #[test]
    fn test_update_fill_rejects_wrong_blank_count_with_error() {
        let mut s = new_state(vec![fill_question(2)]);
        for c in "onlyone".chars() {
            update(&mut s, Msg::Char(c));
        }
        assert_eq!(update(&mut s, Msg::Submit), Cmd::None, "空数不匹配不提交");
        let err = s.error.clone().expect("错误行被设置");
        assert!(err.contains(";"), "错误提示分隔符: {err}");
        // 再次提交前错误清除
        update(&mut s, Msg::Char(';'));
        update(&mut s, Msg::Char('x'));
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["onlyone".to_string(), "x".to_string()]))
        );
        assert!(s.error.is_none(), "进入提交即清错误");
    }

    #[test]
    fn test_update_next_advances_and_finishes() {
        let mut s = new_state(vec![sample_question(), sample_question()]);
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(update(&mut s, Msg::Next), Cmd::None);
        assert_eq!(s.idx, 1, "进入第二题");
        assert!(s.grade.is_none() && s.sel == 0, "状态复位");
        assert!(s.input.is_empty(), "输入行复位");
        assert_eq!(s.revisions, 0, "修订计数复位");
        // 未判分时 Next 无效
        assert_eq!(update(&mut s, Msg::Next), Cmd::None);
        assert_eq!(s.idx, 1);
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Wrong));
        update(&mut s, Msg::Next);
        assert!(s.finished, "最后一题后结束");
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None, "结束后移动无效");
        assert_eq!(update(&mut s, Msg::Quit), Cmd::Quit);
        assert_eq!(update(&mut s, Msg::Next), Cmd::None, "结束后 Next 无效");
    }

    #[test]
    fn test_update_arbitrate_only_fires_on_pending_grade() {
        let mut s = new_state(vec![sample_question()]);
        // 未判分：裁决键无效
        assert_eq!(update(&mut s, Msg::Arbitrate(true)), Cmd::None);
        // 非 Pending 判分：忽略
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(
            update(&mut s, Msg::Arbitrate(false)),
            Cmd::None,
            "已定判不裁决"
        );
        // Pending：转出裁决命令
        apply_grade(&mut s, grade(Verdict::Pending));
        assert_eq!(update(&mut s, Msg::Arbitrate(true)), Cmd::Arbitrate(true));
        assert_eq!(update(&mut s, Msg::Arbitrate(false)), Cmd::Arbitrate(false));
    }

    #[test]
    fn test_update_confidence_records_pre_submit_and_resets_on_next() {
        let mut s = new_state(vec![sample_question()]);
        assert_eq!(update(&mut s, Msg::Confidence(3)), Cmd::None);
        assert_eq!(
            s.self_confidence,
            Some(3),
            "自评落状态（→ Attempt.self_confidence）"
        );
        update(&mut s, Msg::Confidence(9));
        assert_eq!(s.self_confidence, Some(4), "越界钳到 4");
        update(&mut s, Msg::Confidence(0));
        assert_eq!(s.self_confidence, Some(1), "越界钳到 1");
        // 判分后不再改（防误触）
        apply_grade(&mut s, grade(Verdict::Wrong));
        update(&mut s, Msg::Confidence(2));
        assert_eq!(s.self_confidence, Some(1), "判分后锁定");
        // 下一题重置
        update(&mut s, Msg::Next);
        assert_eq!(s.self_confidence, None, "新题重新自评");
        // 填空输入态不吞自评（键位层已路由为字符，状态机双保险）
        let mut fill = new_state(vec![fill_question(1)]);
        update(&mut fill, Msg::Confidence(2));
        assert_eq!(fill.self_confidence, None, "填空输入态不自评");
    }

    #[test]
    fn test_update_select_toggles_multi_and_falls_back_to_cursor() {
        // 多选：Space 切换选中集，提交选中集优先
        let mut s = new_state(vec![multi_question()]);
        update(&mut s, Msg::Select);
        assert_eq!(s.checked, vec![0], "Space 选中光标位");
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::Select);
        assert_eq!(s.checked, vec![0, 1], "多选连按累积");
        update(&mut s, Msg::Select);
        assert_eq!(s.checked, vec![0], "再按取消");
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![0])),
            "选中集优先于光标位"
        );
        // 多选未选中 → 回退光标位（兼容旧行为）
        let mut bare = new_state(vec![multi_question()]);
        update(&mut bare, Msg::MoveDown);
        assert_eq!(
            update(&mut bare, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![1]))
        );
        // 单选：Space 不建选中集，光标即答案
        let mut single = new_state(vec![sample_question()]);
        update(&mut single, Msg::Select);
        assert!(single.checked.is_empty(), "单选 Space 无效");
        update(&mut single, Msg::MoveDown);
        assert_eq!(
            update(&mut single, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![1]))
        );
        // 判分后 Select 忽略 + 下一题清空选中集
        apply_grade(&mut s, grade(Verdict::Correct));
        update(&mut s, Msg::Select);
        assert_eq!(s.checked, vec![0], "判分后不改选中");
        update(&mut s, Msg::Next);
        assert!(s.checked.is_empty(), "下一题清空选中集");
    }

    #[test]
    fn test_update_stop_edit_keeps_input_and_char_resumes() {
        let mut s = new_state(vec![fill_question(1)]);
        for c in "am".chars() {
            update(&mut s, Msg::Char(c));
        }
        assert!(s.editing, "打字即编辑");
        update(&mut s, Msg::StopEdit);
        assert!(!s.editing && s.input == "am", "退出编辑保留输入");
        // 浏览态删改/打字直接恢复编辑
        update(&mut s, Msg::Backspace);
        assert!(s.editing && s.input == "a", "删改即恢复编辑");
        // 判分后不再收字、StopEdit 忽略
        apply_grade(&mut s, grade(Verdict::Correct));
        update(&mut s, Msg::Char('x'));
        assert_eq!(s.input, "a", "判分后不收字符");
        update(&mut s, Msg::StopEdit);
        assert!(s.editing, "判分后 StopEdit 忽略（标志不变）");
        assert_eq!(s.key_ctx(), KeyCtx::Graded, "编辑标志不影响已判分 ctx");
    }
}
