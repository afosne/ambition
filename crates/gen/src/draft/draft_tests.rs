//! DraftQuestion → Question 转换的翻译/作文用例（M5-W2/W3；主体测试留 draft/mod.rs 配对，P-16）

#[cfg(test)]
mod tests {
    use super::super::*;
    use ambition_core::QuestionKind;

    #[test]
    fn test_to_question_builds_translation_with_l1_source_and_l2_reference() {
        let draft: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "translation",
            "stem": "Translate the following sentence into English.",
            "passage": "我喜欢春天。",
            "answers": ["I like spring."],
            "difficulty": 2,
            "tags": ["translation", "cet4"]
        }))
        .expect("翻译草稿");
        let q = draft.to_question().expect("转换");
        let QuestionKind::Translation(body) = &q.kind else {
            panic!("题型错误: {:?}", q.kind);
        };
        assert_eq!(body.source_text, "我喜欢春天。", "源文 = l1（不进 state）");
        assert_eq!(body.reference, "I like spring.", "参考译文 = l2");
        assert_eq!(body.direction.from.0, ambition_core::DEFAULT_L1);
        assert_eq!(body.direction.to.0, "en");

        // 缺参考译文 → Err（不伪造判分参照）
        let no_ref: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "translation",
            "stem": "Translate ...",
            "passage": "你好。",
            "answers": [],
            "difficulty": 2,
            "tags": []
        }))
        .expect("可反序列化");
        assert!(no_ref.to_question().is_err(), "缺 reference → Err");
    }

    #[test]
    fn test_to_question_builds_essay_from_draft_tags_and_passage() {
        let draft: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "essay",
            "stem": "Write an essay in English according to the prompt.",
            "passage": "以“绿色校园”为题写一篇120词左右的短文。",
            "answers": [],
            "difficulty": 3,
            "tags": ["essay", "cet4", "argument"]
        }))
        .expect("作文草稿");
        let q = draft.to_question().expect("转换");
        let QuestionKind::Essay(body) = &q.kind else {
            panic!("题型错误: {:?}", q.kind);
        };
        assert_eq!(
            body.prompt_zh, "以“绿色校园”为题写一篇120词左右的短文。",
            "passage = 中文题面"
        );
        assert_eq!(body.track, "cet4", "track 从 tags 推");
        assert_eq!(body.genre, "argument", "genre 从 tags 推");
        assert_eq!(body.word_range, (120, 180), "cet4 字数红线");
        assert_eq!(body.minutes, 30);
        assert_eq!(body.rubric_id, "gaokao25", "MVP 非雅思轨复用高考25分制");

        // 缺中文题面 → Err（不伪造题面）
        let no_prompt: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "essay",
            "stem": "Write ...",
            "passage": "   ",
            "difficulty": 3,
            "tags": []
        }))
        .expect("可反序列化");
        assert!(no_prompt.to_question().is_err(), "缺题面 → Err");
    }

    #[test]
    fn test_to_question_rejects_malformed_drafts() {
        let base = serde_json::json!({
            "kind": "single_choice",
            "stem": "S.",
            "options": ["a", "b"],
            "answers": ["a"],
            "difficulty": 2,
            "tags": []
        });
        let mut bad = base.clone();
        bad["answers"] = serde_json::json!(["not-an-option"]);
        assert!(
            serde_json::from_value::<DraftQuestion>(bad)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "答案不在选项中 → Err"
        );

        let mut empty_stem = base.clone();
        empty_stem["stem"] = serde_json::json!("   ");
        assert!(
            serde_json::from_value::<DraftQuestion>(empty_stem)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "空题干 → Err"
        );

        let mut dup = base.clone();
        dup["options"] = serde_json::json!(["a", "a"]);
        assert!(
            serde_json::from_value::<DraftQuestion>(dup)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "重复选项 → Err"
        );

        let single_one = serde_json::json!({
            "kind": "single_choice", "stem": "S.", "options": ["a", "b", "c"],
            "answers": ["a", "b"], "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(single_one)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "单选多答 → Err"
        );

        let multi_one = serde_json::json!({
            "kind": "multi_choice", "stem": "S.", "options": ["a", "b"],
            "answers": ["a"], "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(multi_one)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "多选单答 → Err"
        );

        let fill_marker_mismatch = serde_json::json!({
            "kind": "fill_blank", "stem": "She ___ every ___ day.",
            "answers": [], "blanks": [["runs"]], "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(fill_marker_mismatch)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "空位数不匹配 → Err"
        );

        let bad_difficulty = serde_json::json!({
            "kind": "single_choice", "stem": "S.", "options": ["a", "b"],
            "answers": ["a"], "difficulty": 9, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(bad_difficulty)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "难度越界 → Err"
        );
    }
}
