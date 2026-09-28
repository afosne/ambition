//! Jev 跨文件测试夹具（收口前 client/wire 两份逐字复制，2026-09-27 复用合并）

use crate::{ChoiceQ, JevQuestion, JevQuestions, NoulQ, ScoreQ};

/// 三原语齐全的样例判断集（noul/score/choice；客户端与编解码测试共用）
pub(crate) fn sample_questions() -> JevQuestions {
    let mut qs = JevQuestions::new();
    qs.insert(
        "unique_answer".into(),
        JevQuestion::Noul(NoulQ {
            instructions: "Is exactly one option correct?".into(),
        }),
    );
    qs.insert(
        "natural".into(),
        JevQuestion::Score(ScoreQ {
            instructions: "How natural is the sentence?".into(),
            levels: vec!["unnatural".into(), "ok".into(), "natural".into()],
        }),
    );
    qs.insert(
        "error_cause".into(),
        JevQuestion::Choice(ChoiceQ {
            instructions: "Classify the mistake.".into(),
            criteria: [("word_sense".to_string(), "wrong sense".to_string())]
                .into_iter()
                .collect(),
        }),
    );
    qs
}

/// Jev 正常回包：与 `sample_questions` 对应的三原语答案 + usage（decode/路由/客户端测试共用）
pub(crate) fn ok_body() -> serde_json::Value {
    serde_json::json!({
        "model": "jev-1.13.0",
        "answers": {
            "unique_answer": {"type": "noul", "noul": 0.95},
            "natural": {"type": "score", "score": 1.9, "confidence": 0.9,
                        "legend": {"0":"unnatural","1":"ok","2":"natural"},
                        "probabilities": {"0":0.0,"1":0.05,"2":0.95}},
            "error_cause": {"type": "choice", "choice": "word_sense",
                            "probabilities": {"word_sense": 0.9}, "confidence": 0.88}
        },
        "usage": {"input_tokens": 300, "output_tokens": 25}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_questions_fixture_covers_three_primitives() {
        let qs = sample_questions();
        assert_eq!(qs.len(), 3, "三原语各一问");
        assert!(qs.contains_key("unique_answer"));
        assert!(qs.contains_key("natural"));
        assert!(qs.contains_key("error_cause"));
    }

    #[test]
    fn test_ok_body_fixture_is_complete_ok_response() {
        let body = ok_body();
        assert_eq!(body["model"], "jev-1.13.0", "带模型标识");
        assert_eq!(
            body["answers"].as_object().map(|a| a.len()),
            Some(3),
            "三原语答案齐全"
        );
        assert_eq!(body["usage"]["input_tokens"], 300, "带 usage 记账");
    }
}
