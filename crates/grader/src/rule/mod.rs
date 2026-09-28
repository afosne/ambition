//! 一二级规则层（细分域，docs/07-grading §1）：纯规则判定（M1 三题型）+ 判定环子组件（`rule::ring`）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

mod ring;
#[cfg(test)]
mod ring_tests;

use crate::*;

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
        detail: GradeDetail::Items(vec![ItemResult { index: 0, correct }]),
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
            detail: GradeDetail::Items(vec![ItemResult {
                index: 0,
                correct: false,
            }]),
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
        detail: GradeDetail::Items(items),
        error_tags: vec![],
    }
}

/// 规则层分发：M1 三题型全覆盖；复合题型（Cloze/Reading/SevenOfFive/GrammarFill）不在此层判
/// （None → `Grader::grade` 拆子题递归，docs/07 §1 M4 注记）；None = 需要升级到 Jev（M3）
pub fn grade_rule(kind: &QuestionKind, answers: &[u32], fills: &[String]) -> Option<GradeResult> {
    match kind {
        QuestionKind::SingleChoice(body) => Some(grade_single_choice(body, answers)),
        QuestionKind::MultiChoice(body) => grade_multi_choice_rule(body, answers),
        QuestionKind::FillBlank(fill) => Some(grade_fill_blank_rule(fill, fills)),
        // 复合题型：由 Grader::grade 递归拆子题（本层不判）
        QuestionKind::Cloze(_)
        | QuestionKind::Reading(_)
        | QuestionKind::SevenOfFive(_)
        | QuestionKind::GrammarFill(_) => None,
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

    fn cloze_question(blanks: usize) -> Question {
        Question::new(
            "I ___ a student. She ___ to school.".into(),
            QuestionKind::Cloze(ambition_core::ClozeBody {
                passage: "I ___ a student. She ___ to school.".into(),
                blanks: (0..blanks)
                    .map(|_| BlankItem {
                        acceptable: vec!["am".into()],
                    })
                    .collect(),
            }),
            2,
            vec!["grammar".into()],
            ambition_core::Provenance::Seed,
        )
    }

    fn reading_question() -> Question {
        Question::new(
            "Read and answer.".into(),
            QuestionKind::Reading(ambition_core::ReadingBody {
                passage: "Some passage.".into(),
                items: vec![
                    ambition_core::ReadingItem {
                        question: "Q1?".into(),
                        options: choice(vec![0]).options,
                        answer: vec![0],
                    },
                    ambition_core::ReadingItem {
                        question: "Q2?".into(),
                        options: choice(vec![1]).options,
                        answer: vec![1],
                    },
                ],
            }),
            3,
            vec!["reading".into()],
            ambition_core::Provenance::Seed,
        )
    }

    fn seven_of_five_question() -> Question {
        Question::new(
            "Gap fill.".into(),
            QuestionKind::SevenOfFive(ambition_core::SevenOfFiveBody {
                passage: "A ___ B ___ C".into(),
                options: vec!["opt1".into(), "opt2".into(), "opt3".into()],
                answer: vec![0, 2],
            }),
            3,
            vec!["seven".into()],
            ambition_core::Provenance::Seed,
        )
    }

    #[tokio::test]
    async fn test_grade_cloze_recurses_blanks_and_aggregates() {
        let grader = Grader::offline();
        let q = cloze_question(2);
        // 全对
        let ok = grader
            .grade(&q, &Answer::Fill(vec!["am".into(), "am".into()]))
            .await;
        assert_eq!(ok.verdict, Verdict::Correct);
        assert_eq!((ok.score, ok.max), (2.0, 2.0), "子题分累加");
        assert_eq!(ok.graded_by, GradedBy::Rule);
        assert_eq!(ok.items().len(), 2, "两子题两 item");
        // 一对一半错 → Partial
        let partial = grader
            .grade(&q, &Answer::Fill(vec!["am".into(), "wrong".into()]))
            .await;
        assert_eq!(partial.verdict, Verdict::Partial);
        assert_eq!((partial.score, partial.max), (1.0, 2.0));
        assert!(partial.items()[0].correct && !partial.items()[1].correct);
        // 全错 → Wrong
        let wrong = grader
            .grade(&q, &Answer::Fill(vec!["x".into(), "y".into()]))
            .await;
        assert_eq!(wrong.verdict, Verdict::Wrong);
        assert_eq!(wrong.score, 0.0);
        // 作答位数不齐 → 防御性 Pending（不伪造判分）
        let mismatch = grader.grade(&q, &Answer::Fill(vec!["am".into()])).await;
        assert_eq!(mismatch.verdict, Verdict::Pending, "位数不齐防御");
        assert_eq!(mismatch.max, 2.0, "max = 子题数");
    }

    #[tokio::test]
    async fn test_grade_reading_recurses_group_and_aggregates() {
        let grader = Grader::offline();
        let q = reading_question();
        let all_right = grader
            .grade(
                &q,
                &Answer::Group(vec![Answer::Choice(vec![0]), Answer::Choice(vec![1])]),
            )
            .await;
        assert_eq!(all_right.verdict, Verdict::Correct);
        assert_eq!((all_right.score, all_right.max), (2.0, 2.0));
        assert_eq!(all_right.items().len(), 2);
        let one_wrong = grader
            .grade(
                &q,
                &Answer::Group(vec![Answer::Choice(vec![0]), Answer::Choice(vec![0])]),
            )
            .await;
        assert_eq!(one_wrong.verdict, Verdict::Partial);
        assert!(!one_wrong.items()[1].correct, "第二子题错");
        let all_wrong = grader
            .grade(
                &q,
                &Answer::Group(vec![Answer::Choice(vec![2]), Answer::Choice(vec![0])]),
            )
            .await;
        assert_eq!(all_wrong.verdict, Verdict::Wrong);
        assert_eq!(all_wrong.score, 0.0);
        // Group 位数不齐 → 防御 Pending
        let mismatch = grader
            .grade(&q, &Answer::Group(vec![Answer::Choice(vec![0])]))
            .await;
        assert_eq!(mismatch.verdict, Verdict::Pending);
    }

    #[tokio::test]
    async fn test_grade_seven_of_five_recurses_each_gap() {
        let grader = Grader::offline();
        let q = seven_of_five_question();
        let ok = grader.grade(&q, &Answer::Choice(vec![0, 2])).await;
        assert_eq!(ok.verdict, Verdict::Correct);
        assert_eq!((ok.score, ok.max), (2.0, 2.0), "两缺口两子题");
        assert!(ok.items().iter().all(|i| i.correct));
        let half = grader.grade(&q, &Answer::Choice(vec![0, 1])).await;
        assert_eq!(half.verdict, Verdict::Partial);
        assert!(!half.items()[1].correct);
        let wrong = grader.grade(&q, &Answer::Choice(vec![1, 1])).await;
        assert_eq!(wrong.verdict, Verdict::Wrong);
        // Choice 位数不齐 → 防御 Pending
        let mismatch = grader.grade(&q, &Answer::Choice(vec![0])).await;
        assert_eq!(mismatch.verdict, Verdict::Pending);
    }

    #[test]
    fn test_grade_rule_returns_none_for_composite_kinds() {
        // 复合题不在规则层判（None → Grader::grade 递归拆子题，docs/07 §1 M4）
        let cloze = cloze_question(1);
        assert!(grade_rule(&cloze.kind, &[], &["am".into()]).is_none());
        let reading = reading_question();
        assert!(grade_rule(&reading.kind, &[0], &[]).is_none());
        let s75 = seven_of_five_question();
        assert!(grade_rule(&s75.kind, &[0, 2], &[]).is_none());
        let gf = QuestionKind::GrammarFill(ambition_core::GrammarFillBody {
            passage: "p".into(),
            blanks: vec![BlankItem {
                acceptable: vec!["x".into()],
            }],
        });
        assert!(grade_rule(&gf, &[], &["x".into()]).is_none());
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
            half.items().to_vec(),
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
