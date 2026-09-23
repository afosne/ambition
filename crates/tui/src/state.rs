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
    /// 填空输入行（多空用 `;` 分隔）
    pub input: String,
    /// 判分/提交失败的独立错误行（P-8：错误 ≠ Pending）
    pub error: Option<String>,
    /// 本题起始时刻（提交计时 → Timing，假对检测输入）
    pub started_at: Instant,
    /// 修订次数（光标移动/输入删改，Timing.revision_count）
    pub revisions: u32,
    /// 自评把握 1-4（提交前键入 → Attempt.self_confidence，假对检测输入，改进 #10）
    pub self_confidence: Option<u8>,
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
    /// 人工裁决（待裁定态：y=判对 / n=判错）
    Arbitrate(bool),
    /// 自评把握 1-4
    Confidence(u8),
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

/// 构造初始状态（空题库直接 finished）
pub fn new_state(questions: Vec<Question>) -> AppState {
    let finished = questions.is_empty();
    AppState {
        screen: Screen::Practice,
        questions,
        idx: 0,
        sel: 0,
        grade: None,
        finished,
        input: String::new(),
        error: None,
        started_at: Instant::now(),
        revisions: 0,
        self_confidence: None,
    }
}

impl AppState {
    /// 当前题是否处于填空输入态（有题、未判分、题型为填空）
    pub fn is_fill_input(&self) -> bool {
        !self.finished
            && self.grade.is_none()
            && matches!(self.questions[self.idx].kind, QuestionKind::FillBlank(_))
    }

    /// 采集本题计时（提交时调用；毫秒）
    pub fn timing(&self) -> Timing {
        Timing {
            total_ms: self.started_at.elapsed().as_millis() as u64,
            revision_count: self.revisions,
        }
    }
}

/// 写入判分结果（main 在 submit 成功后调用）
pub fn apply_grade(state: &mut AppState, grade: GradeResult) {
    state.grade = Some(grade);
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
        assert!(s.input.is_empty());
        assert!(s.error.is_none());
        assert_eq!(s.revisions, 0);
        assert!(new_state(vec![]).finished, "空题库直接结束");
        assert_eq!(s.screen, Screen::Practice);
    }

    #[test]
    fn test_is_fill_input_gates_char_input() {
        let choice = new_state(vec![sample_question()]);
        assert!(!choice.is_fill_input(), "选择题不吞字符");
        let mut fill = new_state(vec![fill_question(1)]);
        assert!(fill.is_fill_input(), "填空未判分 = 输入态");
        apply_grade(&mut fill, grade(Verdict::Correct));
        assert!(!fill.is_fill_input(), "判分后退出输入态");
        fill.finished = true;
        let mut c2 = choice.clone();
        c2.finished = true;
        assert!(!c2.is_fill_input(), "结束后非输入态");
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
