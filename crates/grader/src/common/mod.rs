//! 判分通用组件（细分域，docs/07-grading）：三方共用辅助（helpers 内容）+ 判词载荷（judges）+ 语言阈值带（thresholds）
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

#[cfg(test)]
pub(crate) mod fixtures;

pub mod judges;
pub mod thresholds;

use crate::*;

/// 规则层不可判定时的 Pending（进 arbitration 桶；confidence=0 标低置信）
pub(crate) fn pending_grade(
    score: f32,
    max: f32,
    items: Vec<ItemResult>,
    error_tags: Vec<ErrorTag>,
) -> GradeResult {
    GradeResult {
        verdict: Verdict::Pending,
        score,
        max,
        confidence: 0.0,
        graded_by: GradedBy::Rule,
        detail: GradeDetail::Items(items),
        error_tags,
    }
}

/// 多选判断集的最小 margin（整体置信 = 最弱判断）
pub(crate) fn min_margin(items: &[ItemJudge]) -> f32 {
    items.iter().map(|i| i.margin).fold(1.0, f32::min)
}

/// graded_by 权重（Human > LlmReview > Jev > Rule；复合题汇总取最重路径，docs/07 §1 M4）
pub(crate) fn heavier_graded_by(current: GradedBy, candidate: GradedBy) -> GradedBy {
    // 路径权重（Human > LlmReview > Jev > Rule；聚合取最重）
    let weight = |g: GradedBy| match g {
        GradedBy::Rule => 0,
        GradedBy::Jev => 1,
        GradedBy::LlmReview => 2,
        GradedBy::Human => 3,
    };
    if weight(candidate) > weight(current) {
        candidate
    } else {
        current
    }
}

/// 拆作答（choice ids / fill 文本；Group 复合题不在本层拆，由递归判分处理）
pub(crate) fn split_answer(answer: &Answer) -> (Vec<u32>, Vec<String>) {
    match answer {
        Answer::Choice(ids) => (ids.clone(), Vec::new()),
        Answer::Fill(values) => (Vec::new(), values.clone()),
        Answer::Group(_) => (Vec::new(), Vec::new()),
        // 文本作答（翻译/作文）本层不拆——composite 分支在 grade() 前置拦截
        Answer::Text(_) => (Vec::new(), Vec::new()),
    }
}

/// 追加 error_cause 判断到集合（按 l2 选语料版）
pub(crate) fn merge_error_cause(lang: &str, mut questions: JevQuestions) -> JevQuestions {
    use crate::corpus::error_cause_question;
    questions.insert("error_cause".into(), error_cause_question(lang));
    questions
}

/// Jev state：只放 l2 内容（题干/选项/参考答案/学生作答，docs/05 §3.5）
pub(crate) fn answer_state(question: &Question, answer: &Answer) -> JevState {
    let mut state = serde_json::Map::new();
    state.insert("stem".into(), serde_json::json!(question.stem));
    state.insert("difficulty".into(), serde_json::json!(question.difficulty));
    match &question.kind {
        QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) => {
            let options: Vec<serde_json::Value> = body
                .options
                .iter()
                .map(|o| serde_json::json!({"id": o.id, "text": o.text}))
                .collect();
            state.insert("options".into(), serde_json::json!(options));
            let reference: Vec<&str> = body
                .options
                .iter()
                .filter(|o| body.answer.contains(&o.id))
                .map(|o| o.text.as_str())
                .collect();
            state.insert("reference_answers".into(), serde_json::json!(reference));
        }
        QuestionKind::FillBlank(fill) => {
            let reference: Vec<&Vec<String>> = fill.blanks.iter().map(|b| &b.acceptable).collect();
            state.insert("reference_answers".into(), serde_json::json!(reference));
        }
        // 复合题：Jev state 只放语篇/结构，子题各自递归判分时会重建自己的 state（docs/07 §1 M4）
        QuestionKind::Cloze(cloze) => {
            let reference: Vec<&Vec<String>> = cloze.blanks.iter().map(|b| &b.acceptable).collect();
            state.insert("passage".into(), serde_json::json!(cloze.passage));
            state.insert("reference_answers".into(), serde_json::json!(reference));
        }
        QuestionKind::GrammarFill(gf) => {
            let reference: Vec<&Vec<String>> = gf.blanks.iter().map(|b| &b.acceptable).collect();
            state.insert("passage".into(), serde_json::json!(gf.passage));
            state.insert("reference_answers".into(), serde_json::json!(reference));
        }
        QuestionKind::Reading(reading) => {
            state.insert("passage".into(), serde_json::json!(reading.passage));
            state.insert(
                "item_questions".into(),
                serde_json::json!(
                    reading
                        .items
                        .iter()
                        .map(|i| &i.question)
                        .collect::<Vec<_>>()
                ),
            );
        }
        QuestionKind::SevenOfFive(s75) => {
            state.insert("passage".into(), serde_json::json!(s75.passage));
            state.insert("options".into(), serde_json::json!(&s75.options));
        }
        // 翻译/作文：本层不建 state（composite 分支自建：只进 l2，源文 l1 永不进，docs/05 §3.5）
        QuestionKind::Translation(_) => {}
    }
    state.insert(
        "student_answer".into(),
        serde_json::to_value(answer).unwrap_or(serde_json::Value::Null),
    );
    JevState(serde_json::Value::Object(state))
}

/// 学生作答的可读形式（choice → 选项号；fill → 分号串；group → 逐子题递归）
pub(crate) fn answer_display(answer: &Answer) -> String {
    match answer {
        Answer::Choice(ids) => ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        Answer::Fill(values) => values.join(" ; "),
        Answer::Group(subs) => subs
            .iter()
            .enumerate()
            .map(|(i, sub)| format!("{}.{}", i + 1, answer_display(sub)))
            .collect::<Vec<_>>()
            .join(" | "),
        Answer::Text(text) => text.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::*;

    #[test]
    fn test_split_answer_separates_choice_and_fill() {
        let (ids, fills) = split_answer(&Answer::Choice(vec![1, 2]));
        assert_eq!(ids, vec![1, 2]);
        assert!(fills.is_empty());
        let (ids2, fills2) = split_answer(&Answer::Fill(vec!["a".into()]));
        assert!(ids2.is_empty());
        assert_eq!(fills2, vec!["a".to_string()]);
    }

    #[test]
    fn test_pending_grade_marks_low_confidence() {
        let grade = pending_grade(1.0, 2.0, vec![], vec![ErrorTag::Tense]);
        assert_eq!(grade.verdict, Verdict::Pending);
        assert_eq!(grade.confidence, 0.0, "低置信标记");
        assert_eq!(grade.graded_by, GradedBy::Rule);
        assert_eq!((grade.score, grade.max), (1.0, 2.0));
        assert_eq!(grade.error_tags, vec![ErrorTag::Tense]);
    }

    #[test]
    fn test_min_margin_takes_weakest_judgment() {
        assert_eq!(min_margin(&[]), 1.0, "空集恒 1");
        assert_eq!(
            min_margin(&[
                ItemJudge {
                    index: 0,
                    belongs: true,
                    margin: 0.9
                },
                ItemJudge {
                    index: 1,
                    belongs: false,
                    margin: 0.2
                },
            ]),
            0.2
        );
    }

    #[test]
    fn test_answer_state_is_l2_only_and_carries_context() {
        let state = answer_state(&choice_question(), &Answer::Choice(vec![1]));
        let text = state.0.to_string();
        assert!(text.contains("She is quick"), "题干入 state");
        assert!(text.contains("reference_answers"), "参考答案入 state");
        assert!(text.contains("student_answer"), "学生作答入 state");
        assert!(
            !text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "无 CJK（l2-only）: {text}"
        );
        let fill_state = answer_state(&fill_question(), &Answer::Fill(vec!["is".into()]));
        assert!(
            fill_state.0.to_string().contains("\"is\""),
            "填空参考入 state"
        );
    }

    #[test]
    fn test_answer_display_renders_choice_ids_and_fill_text() {
        assert_eq!(answer_display(&Answer::Choice(vec![0, 2])), "0,2");
        assert_eq!(
            answer_display(&Answer::Fill(vec!["a".into(), "b".into()])),
            "a ; b"
        );
        assert_eq!(answer_display(&Answer::Choice(vec![])), "", "空选为空串");
        assert_eq!(answer_display(&Answer::Fill(vec![])), "", "空填为空串");
    }

    #[test]
    fn test_heavier_graded_by_picks_strongest_path() {
        use GradedBy::*;
        assert_eq!(heavier_graded_by(Rule, Jev), Jev, "Jev 重于 Rule");
        assert_eq!(heavier_graded_by(Jev, Human), Human, "Human 最重");
        assert_eq!(heavier_graded_by(Human, Rule), Human, "已最重不降级");
        assert_eq!(heavier_graded_by(LlmReview, Jev), LlmReview, "不降级");
        assert_eq!(heavier_graded_by(Rule, Rule), Rule, "相等保持");
    }

    #[test]
    fn test_merge_error_cause_appends_to_questions() {
        let mut questions = JevQuestions::new();
        questions.insert(
            "acceptable".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "fits".into(),
            }),
        );
        let merged = merge_error_cause("en", questions);
        assert!(merged.contains_key("error_cause"));
        assert!(merged.contains_key("acceptable"));
    }
}
