//! engine 跨文件测试夹具（收口前 assembly/practice 四份逐字复制：choice_question、attempt_for；2026-09-27 复用合并）

use ambition_core::{
    Answer, Attempt, AttemptMode, ChoiceBody, OptionItem, Provenance, Qid, Question, QuestionKind,
};

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

/// 样例作答（选择提交、无计时不自评；装配/练习/题包/裁决测试共用）
pub(crate) fn attempt_for(qid: Qid, selected: Vec<u32>, id: &str) -> Attempt {
    Attempt {
        attempt_id: id.into(),
        qid,
        answer: Answer::Choice(selected),
        timing: None,
        self_confidence: None,
        mode: AttemptMode::Practice,
    }
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

    #[test]
    fn test_attempt_for_fixture_is_choice_practice_without_timing() {
        let attempt = attempt_for(Qid("q1".into()), vec![0, 2], "a-1");
        assert_eq!(attempt.attempt_id, "a-1");
        assert!(matches!(attempt.answer, Answer::Choice(ids) if ids == vec![0, 2]));
        assert_eq!(attempt.mode, AttemptMode::Practice);
        assert!(attempt.timing.is_none(), "无计时（非假对基线样本）");
    }
}
