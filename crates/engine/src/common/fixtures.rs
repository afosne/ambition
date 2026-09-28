//! engine 跨文件测试夹具（收口前 assembly/practice 四份逐字复制，2026-09-27 复用合并）

use ambition_core::{ChoiceBody, OptionItem, Provenance, Question, QuestionKind};

/// 单选样例题（装配/练习/题包/裁决测试共用：quick/slow、难度1、vocab 标签）
pub(crate) fn choice_question() -> Question {
    Question::new(
        "She is quick.".into(),
        QuestionKind::SingleChoice(ChoiceBody {
            options: vec![
                OptionItem {
                    id: 0,
                    text: "quick".into(),
                },
                OptionItem {
                    id: 1,
                    text: "slow".into(),
                },
            ],
            answer: vec![0],
        }),
        1,
        vec!["vocab".into()],
        Provenance::Seed,
    )
}

/// 带自定义标签的单选样例题（SRS 入卡按标签分轨测试用：quick/slowly、难度2）
pub(crate) fn choice_question_tagged(tag: &str) -> Question {
    Question::new(
        "She is quick.".into(),
        QuestionKind::SingleChoice(ChoiceBody {
            options: vec![
                OptionItem {
                    id: 0,
                    text: "quick".into(),
                },
                OptionItem {
                    id: 1,
                    text: "slowly".into(),
                },
            ],
            answer: vec![0],
        }),
        2,
        vec![tag.to_string()],
        Provenance::Seed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choice_question_fixture_is_vocab_single_choice() {
        let q = choice_question();
        assert!(
            matches!(q.kind, QuestionKind::SingleChoice(_)),
            "单选夹具（装配/练习测试共用）"
        );
        assert_eq!(q.tags, vec!["vocab".to_string()], "vocab 标签");
        assert_eq!(q.difficulty, 1);
    }

    #[test]
    fn test_choice_question_tagged_fixture_carries_custom_tag() {
        let q = choice_question_tagged("grammar");
        assert_eq!(q.tags, vec!["grammar".to_string()], "标签由调用方定");
        assert_eq!(q.difficulty, 2);
    }
}
