//! Ambition grader：三级判题流水线（docs/07-grading）
//! M1 规则层（本文件纯函数）；M3 落地 `pipeline::Grader`（规则 → Jev → LLM 复核/待裁定桶）
//! 与 `corpus`（assets/jev/<l2>.json 判分语料 + 错因归类口）。

mod corpus;
mod pipeline;
// 三级流水线分层（docs/01 §组织：文件 ≤500 行；impl Grader 跨模块拆分，字段/辅助需 pub(crate)）
mod arbitrate;
mod fill;
mod helpers;
mod judges;
mod rule;
mod thresholds;

pub use arbitrate::*;
pub use corpus::{
    ERROR_TAGS_VERSION, corpus_version, error_cause_question, error_cause_questions,
    error_criteria, tag_error_cause,
};
pub(crate) use helpers::*;
pub use judges::*;
pub use pipeline::*;
pub use thresholds::*;

// 子模块（thresholds/judges/helpers/rule/fill/arbitrate/pipeline）经 `use super::*` 共享本组导入
use ambition_core::{
    Answer, ChoiceBody, ErrorTag, FillBody, GradeResult, GradedBy, ItemResult, Question,
    QuestionKind, Verdict, normalize_text,
};
use ambition_jev::Route;
use ambition_jev::{
    JevClient, JevError, JevQuestion, JevQuestions, JevState, NoulQ, route_from_margin,
};
use ambition_llm::{GenRequest, LlmError, LlmKind, LlmRouter};

/// 比对规范化：去首尾空白标点、可选忽略冠词（按 track 配置）
pub fn normalize_for_compare(input: &str, ignore_articles: bool) -> String {
    let base = normalize_text(input);
    if !ignore_articles {
        return base;
    }
    base.split_whitespace()
        .filter(|w| !matches!(*w, "a" | "an" | "the"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn sorted_set(ids: &[u32]) -> Vec<u32> {
    let mut v = ids.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

/// 单选：选项集合精确比对（0 成本、0 延迟）
pub fn grade_single_choice(body: &ChoiceBody, selected: &[u32]) -> GradeResult {
    let correct = sorted_set(selected) == sorted_set(&body.answer);
    GradeResult {
        verdict: if correct {
            Verdict::Correct
        } else {
            Verdict::Wrong
        },
        score: if correct { 1.0 } else { 0.0 },
        max: 1.0,
        confidence: 1.0,
        graded_by: GradedBy::Rule,
        items: vec![ItemResult { index: 0, correct }],
        error_tags: vec![],
    }
}

/// 多选规则层：全对→Correct；含错误选项→Wrong；仅漏选（部分分需 Jev）→ None 升级
pub fn grade_multi_choice_rule(body: &ChoiceBody, selected: &[u32]) -> Option<GradeResult> {
    let sel = sorted_set(selected);
    let ans = sorted_set(&body.answer);
    if sel == ans {
        return Some(grade_single_choice(body, selected));
    }
    if sel.iter().any(|id| !ans.contains(id)) {
        return Some(GradeResult {
            verdict: Verdict::Wrong,
            score: 0.0,
            max: 1.0,
            confidence: 1.0,
            graded_by: GradedBy::Rule,
            items: vec![ItemResult {
                index: 0,
                correct: false,
            }],
            error_tags: vec![],
        });
    }
    // 严格子集：漏选部分分，规则层不判 → M3 Jev 逐项 noul
    None
}

/// 填空：逐空规范化比可接受答案表；M1 规则层全对/否则 Wrong（部分分归 Jev）
pub fn grade_fill_blank_rule(fill: &FillBody, answers: &[String]) -> GradeResult {
    let items: Vec<ItemResult> = fill
        .blanks
        .iter()
        .enumerate()
        .map(|(index, blank)| {
            let correct = answers
                .get(index)
                .is_some_and(|given| blank.acceptable.iter().any(|ok| ok == given));
            ItemResult { index, correct }
        })
        .collect();
    let score = items.iter().filter(|i| i.correct).count() as f32;
    let all = score == items.len() as f32;
    GradeResult {
        verdict: if all {
            Verdict::Correct
        } else {
            Verdict::Wrong
        },
        score,
        max: items.len() as f32,
        confidence: 1.0,
        graded_by: GradedBy::Rule,
        items,
        error_tags: vec![],
    }
}

/// 规则层分发：M1 题型全覆盖；None = 需要升级到 Jev（M3）
pub fn grade_rule(kind: &QuestionKind, answers: &[u32], fills: &[String]) -> Option<GradeResult> {
    match kind {
        QuestionKind::SingleChoice(body) => Some(grade_single_choice(body, answers)),
        QuestionKind::MultiChoice(body) => grade_multi_choice_rule(body, answers),
        QuestionKind::FillBlank(fill) => Some(grade_fill_blank_rule(fill, fills)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{BlankItem, OptionItem};

    fn choice(answer: Vec<u32>) -> ChoiceBody {
        ChoiceBody {
            options: vec![
                OptionItem {
                    id: 0,
                    text: "quick".into(),
                },
                OptionItem {
                    id: 1,
                    text: "slow".into(),
                },
                OptionItem {
                    id: 2,
                    text: "fast".into(),
                },
            ],
            answer,
        }
    }

    fn fill(acceptable: Vec<Vec<&str>>) -> FillBody {
        FillBody {
            sentence: "She is ___ student, ___ ok.".into(),
            blanks: acceptable
                .into_iter()
                .map(|a| BlankItem {
                    acceptable: a.into_iter().map(String::from).collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_normalize_for_compare_handles_articles() {
        assert_eq!(normalize_for_compare("The Student!", false), "the student");
        assert_eq!(normalize_for_compare("The Student!", true), "student");
        assert_eq!(normalize_for_compare("an apple", true), "apple");
        assert_eq!(normalize_for_compare("apple", false), "apple");
    }

    #[test]
    fn test_grade_single_choice_exact_match() {
        let body = choice(vec![0]);
        let ok = grade_single_choice(&body, &[0]);
        assert_eq!(ok.verdict, Verdict::Correct);
        assert_eq!(ok.score, 1.0);
        assert_eq!(ok.graded_by, GradedBy::Rule);
        let no = grade_single_choice(&body, &[1]);
        assert_eq!(no.verdict, Verdict::Wrong);
        assert_eq!(no.score, 0.0);
        let none_sel = grade_single_choice(&body, &[]);
        assert_eq!(none_sel.verdict, Verdict::Wrong);
    }

    #[test]
    fn test_grade_multi_choice_rule_paths() {
        let body = choice(vec![0, 2]);
        // 全对
        assert_eq!(
            grade_multi_choice_rule(&body, &[2, 0]).map(|g| g.verdict),
            Some(Verdict::Correct)
        );
        // 含错误选项 → 确定判错
        assert_eq!(
            grade_multi_choice_rule(&body, &[0, 1]).map(|g| g.verdict),
            Some(Verdict::Wrong)
        );
        // 严格子集（漏选）→ 升级 Jev，规则层返回 None
        assert!(grade_multi_choice_rule(&body, &[0]).is_none());
    }

    #[test]
    fn test_grade_fill_blank_rule_per_blank() {
        let body = fill(vec![vec!["a", "an"], vec!["ok", "fine"]]);
        let all = grade_fill_blank_rule(&body, &["a".to_string(), "fine".to_string()]);
        assert_eq!(all.verdict, Verdict::Correct);
        assert_eq!(all.score, 2.0);
        assert_eq!(all.max, 2.0);

        let half = grade_fill_blank_rule(&body, &["a".to_string(), "bad".to_string()]);
        assert_eq!(half.verdict, Verdict::Wrong);
        assert_eq!(half.score, 1.0);
        assert_eq!(
            half.items,
            vec![
                ItemResult {
                    index: 0,
                    correct: true
                },
                ItemResult {
                    index: 1,
                    correct: false
                }
            ]
        );

        let missing = grade_fill_blank_rule(&body, &[]);
        assert_eq!(missing.verdict, Verdict::Wrong);
        assert_eq!(missing.score, 0.0);
    }

    #[test]
    fn test_sorted_set_sorts_and_dedups() {
        assert_eq!(sorted_set(&[2, 0, 2, 1]), vec![0, 1, 2]);
        assert_eq!(sorted_set(&[]), Vec::<u32>::new());
    }

    #[test]
    fn test_grade_rule_dispatches_by_kind() {
        let single = QuestionKind::SingleChoice(choice(vec![1]));
        assert_eq!(
            grade_rule(&single, &[1], &[]).map(|g| g.verdict),
            Some(Verdict::Correct)
        );
        let multi = QuestionKind::MultiChoice(choice(vec![0, 2]));
        assert!(grade_rule(&multi, &[0], &[]).is_none()); // 漏选升级
        let fb = QuestionKind::FillBlank(fill(vec![vec!["ok"], vec!["fine"]]));
        assert_eq!(
            grade_rule(&fb, &[], &["ok".to_string(), "fine".to_string()]).map(|g| g.verdict),
            Some(Verdict::Correct)
        );
    }
}
