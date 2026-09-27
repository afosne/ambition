//! 界面状态机数据：Screen/AppState/Msg/Cmd + 构造与视图辅助（docs/03 §5.1）

use super::*;

/// 屏幕（M1 只有答题流；完整 Screen 集见 docs/03-engine-ui）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Practice,
}

/// 应用状态
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub screen: Screen,
    pub questions: Vec<Question>,
    pub idx: usize,
    pub sel: usize,
    pub grade: Option<GradeResult>,
    pub finished: bool,
    /// 每空一位的输入文本（与题面 blanks 等长；多空分行作答，废除 `;` 分隔）
    pub inputs: Vec<String>,
    /// 当前编辑的空位（0 基；Tab/→/↓ 切换，blank_sel 仅填空待答时有效）
    pub blank_sel: usize,
    /// 判分/提交失败的独立错误行（P-8：错误 ≠ Pending）
    pub error: Option<String>,
    /// 本题起始时刻（提交计时 → Timing，假对检测输入）
    pub started_at: Instant,
    /// 修订次数（光标移动/输入删改，Timing.revision_count）
    pub revisions: u32,
    /// 自评把握 1-4（提交前键入 → Attempt.self_confidence，假对检测输入，改进 #10）
    pub self_confidence: Option<u8>,
    /// 填空编辑态（打字即进入；Esc 退出保留文本——keys.rs KeyCtx::Fill{editing}）
    pub editing: bool,
    /// 选择题已选中集合（Space 切换；空 = 未显式选中，提交回退光标位）
    pub checked: Vec<u32>,
    /// 待裁定裁决高亮（0=判对 1=判错；←/→ 切换，Enter 确认）
    pub arbitration_sel: u8,
}

/// 输入消息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    MoveDown,
    MoveUp,
    Submit,
    Next,
    Quit,
    /// 填空输入行追加字符
    Char(char),
    /// 填空输入行退格
    Backspace,
    /// 人工裁决：确认当前高亮的待裁定裁决（arbitration_sel 0=判对 / 1=判错 → Cmd::Arbitrate）
    ConfirmArbitrate,
    /// 自评把握 1-4
    Confidence(u8),
    /// Space 选中/取消（选择题；单选无意义，多选=切换）
    Select,
    /// Esc 退出编辑（保留已输入；打字直接恢复编辑）
    StopEdit,
}

/// 状态机输出（Quit/Submit 由 main 执行副作用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    None,
    Quit,
    Submit(Answer),
    /// 人工裁决（main 调 engine.resolve_pending）
    Arbitrate(bool),
}

/// 题目空位数（填空类 = blanks 数；其它题型 = 0——inputs 长度据此重建；M4 题型补全含完形/语法填空）
pub(crate) fn blank_count(kind: &QuestionKind) -> usize {
    kind.blank_count()
}

/// 构造初始状态（空题库直接 finished）
pub fn new_state(questions: Vec<Question>) -> AppState {
    let finished = questions.is_empty();
    let inputs = questions
        .first()
        .map(|q| vec![String::new(); blank_count(&q.kind)])
        .unwrap_or_default();
    AppState {
        screen: Screen::Practice,
        questions,
        idx: 0,
        sel: 0,
        grade: None,
        finished,
        inputs,
        blank_sel: 0,
        error: None,
        started_at: Instant::now(),
        revisions: 0,
        self_confidence: None,
        editing: false,
        checked: Vec::new(),
        arbitration_sel: 0,
    }
}

impl AppState {
    /// 当前题是否等待填空作答（有题、未判分、题型为填空）
    pub fn is_fill_pending(&self) -> bool {
        !self.finished && self.grade.is_none() && self.questions[self.idx].kind.is_fill()
    }

    /// 键位上下文（keys.rs 唯一路由入参；键位表只此一份）
    pub fn key_ctx(&self) -> KeyCtx {
        if self.finished {
            return KeyCtx::Finished;
        }
        match &self.grade {
            Some(g) if g.verdict == ambition_core::Verdict::Pending => KeyCtx::Pending,
            Some(_) => KeyCtx::Graded,
            None if matches!(self.questions[self.idx].kind, QuestionKind::FillBlank(_)) => {
                KeyCtx::Fill {
                    editing: self.editing,
                }
            }
            None => KeyCtx::Choice,
        }
    }

    /// 采集本题计时（提交时调用；毫秒）
    pub fn timing(&self) -> Timing {
        Timing {
            total_ms: self.started_at.elapsed().as_millis() as u64,
            revision_count: self.revisions,
        }
    }
}

/// 写入判分结果（main 在 submit 成功后调用；裁决高亮复位到默认档）
pub fn apply_grade(state: &mut AppState, grade: GradeResult) {
    state.grade = Some(grade);
    state.arbitration_sel = 0;
}

/// 写入独立错误行（main 在 submit 失败后调用；P-8：绝不降级成 Pending 假判分）
pub fn apply_error(state: &mut AppState, message: String) {
    state.error = Some(message);
}

/// 内置示例题（测试夹具；运行时题库来自 seed，docs/16-config-i18n §seed）
pub fn sample_question() -> Question {
    Question::new(
        "She is ___ university student.".into(),
        QuestionKind::SingleChoice(ambition_core::ChoiceBody {
            options: vec![
                ambition_core::OptionItem {
                    id: 0,
                    text: "a".into(),
                },
                ambition_core::OptionItem {
                    id: 1,
                    text: "an".into(),
                },
                ambition_core::OptionItem {
                    id: 2,
                    text: "the".into(),
                },
                ambition_core::OptionItem {
                    id: 3,
                    text: "two".into(),
                },
            ],
            answer: vec![1],
        }),
        1,
        vec!["article".into()],
        ambition_core::Provenance::Seed,
    )
}

/// 填空题各空的可接受答案表（渲染正确答案用；非填空 → 空）
pub fn fill_answers(kind: &QuestionKind) -> Vec<Vec<String>> {
    match kind {
        QuestionKind::FillBlank(fill) => fill.blanks.iter().map(|b| b.acceptable.clone()).collect(),
        _ => Vec::new(),
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

    #[test]
    fn test_new_state_initializes_position() {
        let s = new_state(vec![sample_question()]);
        assert_eq!(s.idx, 0);
        assert_eq!(s.sel, 0);
        assert!(s.grade.is_none());
        assert!(!s.finished);
        assert!(s.inputs.is_empty(), "选择题无输入位");
        assert_eq!(s.blank_sel, 0);
        assert!(s.error.is_none());
        assert_eq!(s.revisions, 0);
        assert!(new_state(vec![]).finished, "空题库直接结束");
        assert_eq!(s.screen, Screen::Practice);
    }

    #[test]
    fn test_blank_count_by_kind() {
        assert_eq!(blank_count(&sample_question().kind), 0, "选择题 0 位");
        assert_eq!(blank_count(&fill_question(1).kind), 1, "单词 1 位");
        assert_eq!(blank_count(&fill_question(2).kind), 2, "多空 2 位");
        assert_eq!(
            new_state(vec![fill_question(2)]).inputs.len(),
            2,
            "构造时按空数建输入位"
        );
    }

    #[test]
    fn test_is_fill_pending_gates_char_input() {
        let choice = new_state(vec![sample_question()]);
        assert!(!choice.is_fill_pending(), "选择题不吞字符");
        let mut fill = new_state(vec![fill_question(1)]);
        assert!(fill.is_fill_pending(), "填空未判分 = 待答");
        assert_eq!(
            update(&mut fill, Msg::Char('a')),
            Cmd::None,
            "打字直接进编辑"
        );
        assert!(fill.editing, "首键进入编辑态");
        assert_eq!(update(&mut fill, Msg::StopEdit), Cmd::None, "Esc 退出编辑");
        assert!(!fill.editing && fill.inputs[0] == "a", "退出编辑保留输入");
        update(&mut fill, Msg::Char('b'));
        assert!(fill.editing && fill.inputs[0] == "ab", "打字直接恢复编辑");
        apply_grade(&mut fill, grade(Verdict::Correct));
        assert!(!fill.is_fill_pending(), "判分后退出待答");
        fill.finished = true;
        assert!(!fill.is_fill_pending(), "结束后非待答");
    }

    #[test]
    fn test_key_ctx_routes_states() {
        let choice = new_state(vec![sample_question()]);
        assert_eq!(choice.key_ctx(), KeyCtx::Choice);
        let mut fill = new_state(vec![fill_question(1)]);
        assert_eq!(
            fill.key_ctx(),
            KeyCtx::Fill { editing: false },
            "打字前未编辑（首键直接进编辑）"
        );
        fill.editing = true;
        assert_eq!(fill.key_ctx(), KeyCtx::Fill { editing: true });
        apply_grade(&mut fill, grade(Verdict::Pending));
        assert_eq!(fill.key_ctx(), KeyCtx::Pending, "待裁定单独上下文");
        apply_grade(&mut fill, grade(Verdict::Correct));
        assert_eq!(fill.key_ctx(), KeyCtx::Graded);
        fill.finished = true;
        assert_eq!(fill.key_ctx(), KeyCtx::Finished);
    }

    #[test]
    fn test_apply_error_keeps_grade_none_and_is_overwritten() {
        let mut s = new_state(vec![sample_question()]);
        assert!(s.grade.is_none() && s.error.is_none());
        apply_error(&mut s, "store: disk full".into());
        assert!(s.grade.is_none(), "错误绝不落成 Pending 判分（P-8）");
        assert_eq!(s.error.as_deref(), Some("store: disk full"));
        // 成功路径不写错误
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(s.grade.as_ref().map(|g| g.verdict), Some(Verdict::Correct));
        assert!(s.error.is_some(), "错误行独立存活直到下一次提交清除");
        apply_error(&mut s, "second failure".into());
        assert_eq!(s.error.as_deref(), Some("second failure"), "后写覆盖");
    }

    #[test]
    fn test_timing_collects_elapsed_and_revisions() {
        let mut s = new_state(vec![sample_question()]);
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::MoveUp);
        let t = s.timing();
        assert!(t.total_ms < 60_000, "耗时合理: {}", t.total_ms);
        assert_eq!(t.revision_count, 2, "修订次数来自移动/输入");
        // 复位后归零
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Correct));
        update(&mut s, Msg::Next);
        let t2 = s.timing();
        assert_eq!(t2.revision_count, 0, "下一题重新计数");
        assert!(t2.total_ms <= t.total_ms, "计时重置");
    }

    #[test]
    fn test_sample_question_is_single_choice_with_stable_qid() {
        let q = sample_question();
        assert!(matches!(q.kind, QuestionKind::SingleChoice(_)));
        assert_eq!(q.qid, sample_question().qid, "同内容同 qid");
        assert!(q.tags.contains(&"article".to_string()));
        assert_eq!(q.provenance, Provenance::Seed);
    }

    #[test]
    fn test_apply_grade_sets_and_overrides_grade() {
        let mut s = new_state(vec![sample_question()]);
        assert!(s.grade.is_none());
        apply_grade(&mut s, grade(Verdict::Wrong));
        assert_eq!(s.grade.as_ref().map(|g| g.verdict), Some(Verdict::Wrong));
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(
            s.grade.as_ref().map(|g| g.verdict),
            Some(Verdict::Correct),
            "可覆盖"
        );
        assert!(s.error.is_none(), "成功路径不写错误行");
    }

    #[test]
    fn test_fill_answers_exposes_acceptable_sets() {
        let single = fill_answers(&fill_question(1).kind);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0], vec!["am".to_string(), "by".to_string()]);
        let multi = fill_answers(&fill_question(2).kind);
        assert_eq!(multi.len(), 2, "两空两组答案");
        assert!(
            fill_answers(&sample_question().kind).is_empty(),
            "非填空为空"
        );
    }
}
