//! 判分语料加载：`data/jev/error_tags.json` 内嵌 + 校验（docs/05 §3：措辞即判分口径，变更必升 version）
//! criteria 键 = `core::ErrorTag` snake_case 全集；description 全英文（中文永不进 Jev 请求）。

use std::collections::BTreeMap;
use std::sync::OnceLock;

use ambition_core::ErrorTag;
use ambition_jev::{ChoiceQ, JevAnswer, JevAnswers, JevQuestion, JevQuestions};

/// 代码侧语料版本（与 json 文件 `version` 同步——测试断言，改措辞必须同时改这里）
pub const ERROR_TAGS_VERSION: &str = "1.0.0";

const ERROR_TAGS_JSON: &str = include_str!("../../../data/jev/error_tags.json");

/// 解析语料文件（损坏 → 空对象，criteria 变空表；error_cause_question 兜底可见）
fn corpus_value() -> &'static serde_json::Value {
    static PARSED: OnceLock<serde_json::Value> = OnceLock::new();
    PARSED.get_or_init(|| {
        serde_json::from_str(ERROR_TAGS_JSON).unwrap_or_else(|_| serde_json::json!({}))
    })
}

/// 文件内 version 字段（与 ERROR_TAGS_VERSION 比对的取证口）
pub fn corpus_version() -> String {
    corpus_value()
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// criteria 表（28 键，BTreeMap 稳定顺序；解析失败 = 空表）
pub fn error_criteria() -> &'static BTreeMap<String, String> {
    static CRITERIA: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    CRITERIA.get_or_init(|| {
        let mut out = BTreeMap::new();
        if let Some(obj) = corpus_value().get("criteria").and_then(|c| c.as_object()) {
            for (key, value) in obj {
                if let Some(text) = value.as_str() {
                    out.insert(key.clone(), text.to_string());
                }
            }
        }
        out
    })
}

/// 组装 error_cause 判断（choice：28 类措辞即 criteria；正确=no_error、无类=other）
pub fn error_cause_question() -> JevQuestion {
    JevQuestion::Choice(ChoiceQ {
        instructions: "Classify the mistake in the student's answer. If the answer is correct choose no_error; if the mistake fits none of the categories choose other.".into(),
        criteria: error_criteria().clone(),
    })
}

/// 批量组装判断集（error_cause + 调用方附加问题；BTreeMap 保序）
pub fn error_cause_questions(extra: Vec<(String, JevQuestion)>) -> JevQuestions {
    let mut questions = JevQuestions::new();
    questions.insert("error_cause".into(), error_cause_question());
    for (id, question) in extra {
        questions.insert(id, question);
    }
    questions
}

/// 唯一错因归类口（docs/07 §3）：`error_cause` choice → ErrorTag。
/// confidence < accept 不采信（灰区/低置信不出标签）；未知 key tracing::warn 跳过（不 panic）。
pub fn tag_error_cause(answers: &JevAnswers, accept: f32) -> Vec<ErrorTag> {
    let Some(answer) = answers.get("error_cause") else {
        return Vec::new();
    };
    let JevAnswer::Choice {
        choice, confidence, ..
    } = answer
    else {
        return Vec::new();
    };
    if (*confidence as f32) < accept {
        return Vec::new();
    }
    match ErrorTag::parse_tag(choice) {
        Some(tag) => vec![tag],
        None => {
            tracing::warn!(key = %choice, "unknown error_cause key from Jev, skipping tag");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice_answer(choice: &str, confidence: f64) -> (String, JevAnswer) {
        (
            "error_cause".into(),
            JevAnswer::Choice {
                choice: choice.into(),
                probabilities: BTreeMap::new(),
                confidence,
            },
        )
    }

    #[test]
    fn test_corpus_value_parses_embedded_file() {
        let value = corpus_value();
        assert!(
            value.get("criteria").and_then(|c| c.as_object()).is_some(),
            "内嵌文件含 criteria 对象"
        );
        assert!(
            std::ptr::eq(value, corpus_value()),
            "OnceLock 单例（两次引用同一静态）"
        );
    }

    #[test]
    fn test_corpus_version_matches_code_constant() {
        assert_eq!(
            corpus_version(),
            ERROR_TAGS_VERSION,
            "文件 version 与代码常量同步（措辞即口径，改必双改）"
        );
        assert_eq!(
            corpus_value().get("schema_version"),
            Some(&serde_json::json!(1))
        );
    }

    #[test]
    fn test_error_criteria_covers_all_tags_and_is_english_only() {
        let criteria = error_criteria();
        assert_eq!(criteria.len(), 28, "28 类全集");
        for tag in ErrorTag::all() {
            let desc = criteria
                .get(tag.as_str())
                .unwrap_or_else(|| panic!("缺 criteria: {}", tag.as_str()));
            assert!(!desc.is_empty(), "描述非空: {}", tag.as_str());
            assert!(
                !desc.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "criteria 全英文（中文永不进 Jev）: {desc}"
            );
        }
        assert!(criteria.get("mystery").is_none(), "无野键");
    }

    #[test]
    fn test_error_cause_question_encodes_choice_with_criteria() {
        match error_cause_question() {
            JevQuestion::Choice(q) => {
                assert_eq!(q.criteria.len(), 28, "criteria 随包");
                assert!(q.instructions.contains("no_error"), "指令含兜底键");
            }
            other => panic!("应为 choice: {other:?}"),
        }
    }

    #[test]
    fn test_error_cause_questions_appends_extra_with_error_cause_first() {
        let extra = (
            "acceptable".to_string(),
            JevQuestion::Noul(ambition_jev::NoulQ {
                instructions: "fits".into(),
            }),
        );
        let questions = error_cause_questions(vec![extra]);
        assert!(questions.contains_key("error_cause"), "error_cause 恒在");
        assert!(questions.contains_key("acceptable"), "附加问题并入");
        assert_eq!(questions.len(), 2);
        let bare = error_cause_questions(vec![]);
        assert_eq!(bare.len(), 1, "无附加 = 单判断");
    }

    #[test]
    fn test_tag_error_cause_accepts_confident_known_key() {
        let (id, answer) = choice_answer("collocation", 0.91);
        let answers: JevAnswers = std::iter::once((id, answer)).collect();
        assert_eq!(
            tag_error_cause(&answers, 0.75),
            vec![ErrorTag::Collocation],
            "高置信已知键 → 标签"
        );
    }

    #[test]
    fn test_tag_error_cause_skips_low_confidence_unknown_and_missing() {
        let (id, low) = choice_answer("tense", 0.5);
        let answers: JevAnswers = std::iter::once((id, low)).collect();
        assert!(tag_error_cause(&answers, 0.75).is_empty(), "灰区不采信");
        let (id, unknown) = choice_answer("mystery_tag", 0.99);
        let answers: JevAnswers = std::iter::once((id, unknown)).collect();
        assert!(
            tag_error_cause(&answers, 0.75).is_empty(),
            "未知键 warn 跳过"
        );
        assert!(
            tag_error_cause(&JevAnswers::new(), 0.75).is_empty(),
            "缺 error_cause"
        );
        let (id, wrong_kind) = ("error_cause".to_string(), JevAnswer::Noul { yes_prob: 0.9 });
        let answers: JevAnswers = std::iter::once((id, wrong_kind)).collect();
        assert!(
            tag_error_cause(&answers, 0.75).is_empty(),
            "非 choice 原语忽略"
        );
    }
}
