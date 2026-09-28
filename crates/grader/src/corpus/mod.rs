//! 判分语料加载：`crates/grader/assets/jev/<l2>.json` 内嵌（按 l2 分版、缺版回退 en）+ 校验
//! （docs/05 §3：措辞即判分口径，变更必升 version——全部语言文件与代码常量同步）
//! criteria 键 = `core::ErrorTag` snake_case 全集；instructions/description 语言 = 该版 l2（l1 永不进 Jev 请求）。

use std::collections::BTreeMap;
use std::sync::OnceLock;

use ambition_core::ErrorTag;
use ambition_jev::{ChoiceQ, JevAnswer, JevAnswers, JevQuestion, JevQuestions};

/// 代码侧语料版本（与**全部语言文件** `version` 同步——测试逐版断言，改措辞必须同时改这里）
pub const ERROR_TAGS_VERSION: &str = "1.1.0";

/// zh 谱系语种（选 zh 版语料）
fn is_zh(lang: &str) -> bool {
    matches!(lang, "zh" | "zh-Hans" | "zh-Hant")
}

/// 语料源文件（按 l2 分版，缺版回退 en——include_str 编译期内嵌，不读运行时文件）
fn corpus_json(lang: &str) -> &'static str {
    if is_zh(lang) {
        include_str!("../../assets/jev/zh.json")
    } else {
        include_str!("../../assets/jev/en.json")
    }
}

/// 解析语料文件（每语种一个单例；损坏 → 空对象，criteria 变空表；error_cause_question 兜底可见）
fn corpus_value(lang: &str) -> &'static serde_json::Value {
    static EN: OnceLock<serde_json::Value> = OnceLock::new();
    static ZH: OnceLock<serde_json::Value> = OnceLock::new();
    let slot = if is_zh(lang) { &ZH } else { &EN };
    let json = corpus_json(lang);
    slot.get_or_init(|| serde_json::from_str(json).unwrap_or_else(|_| serde_json::json!({})))
}

/// 文件内 version 字段（与 ERROR_TAGS_VERSION 比对的取证口）
pub fn corpus_version(lang: &str) -> String {
    corpus_value(lang)
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// criteria 表（28 键，BTreeMap 稳定顺序；按 l2 选版、缺版回退 en；解析失败 = 空表）
pub fn error_criteria(lang: &str) -> &'static BTreeMap<String, String> {
    static EN: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    static ZH: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    let slot = if is_zh(lang) { &ZH } else { &EN };
    slot.get_or_init(|| {
        let mut out = BTreeMap::new();
        if let Some(obj) = corpus_value(lang)
            .get("criteria")
            .and_then(|c| c.as_object())
        {
            for (key, value) in obj {
                if let Some(text) = value.as_str() {
                    out.insert(key.clone(), text.to_string());
                }
            }
        }
        out
    })
}

/// 组装 error_cause 判断（choice：28 类措辞即 criteria、instructions 随语料选版；正确=no_error、无类=other）
pub fn error_cause_question(lang: &str) -> JevQuestion {
    let instructions = corpus_value(lang)
        .get("instructions")
        .and_then(|v| v.as_str())
        .or_else(|| {
            corpus_value("en")
                .get("instructions")
                .and_then(|v| v.as_str())
        })
        .unwrap_or_default();
    JevQuestion::Choice(ChoiceQ {
        instructions: instructions.into(),
        criteria: error_criteria(lang).clone(),
    })
}

/// 批量组装判断集（error_cause + 调用方附加问题；BTreeMap 保序）
pub fn error_cause_questions(lang: &str, extra: Vec<(String, JevQuestion)>) -> JevQuestions {
    let mut questions = JevQuestions::new();
    questions.insert("error_cause".into(), error_cause_question(lang));
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
    fn test_corpus_json_picks_edition_by_lang_with_fallback() {
        assert!(corpus_json("en").contains("\"lang\": \"en\""), "en 版");
        assert!(corpus_json("zh").contains("\"lang\": \"zh\""), "zh 版");
        assert!(
            corpus_json("zh-Hans").contains("\"lang\": \"zh\""),
            "zh 谱系同版"
        );
        assert!(
            std::ptr::eq(corpus_json("ja"), corpus_json("en")),
            "未知语种回退 en 版"
        );
    }

    #[test]
    fn test_is_zh_matches_zh_family_only() {
        assert!(is_zh("zh"));
        assert!(is_zh("zh-Hans"));
        assert!(is_zh("zh-Hant"));
        assert!(!is_zh("en"), "en 不归 zh");
        assert!(!is_zh("ja"), "其它语种不归 zh");
    }

    #[test]
    fn test_corpus_value_parses_embedded_file() {
        for lang in ["en", "zh"] {
            let value = corpus_value(lang);
            assert!(
                value.get("criteria").and_then(|c| c.as_object()).is_some(),
                "内嵌文件含 criteria 对象: {lang}"
            );
            assert!(
                std::ptr::eq(value, corpus_value(lang)),
                "OnceLock 单例（两次引用同一静态）: {lang}"
            );
            assert_eq!(
                value.get("lang"),
                Some(&serde_json::json!(lang)),
                "lang 字段与版本一致: {lang}"
            );
            assert!(
                value
                    .get("instructions")
                    .and_then(|i| i.as_str())
                    .is_some_and(|s| !s.is_empty()),
                "instructions 随语料（进 Jev 请求）: {lang}"
            );
        }
    }

    #[test]
    fn test_corpus_version_matches_code_constant_for_every_edition() {
        for lang in ["en", "zh"] {
            assert_eq!(
                corpus_version(lang),
                ERROR_TAGS_VERSION,
                "文件 version 与代码常量同步（措辞即口径，改必全部语言版双改）: {lang}"
            );
            assert_eq!(
                corpus_value(lang).get("schema_version"),
                Some(&serde_json::json!(1))
            );
        }
    }

    #[test]
    fn test_error_criteria_covers_all_tags_in_every_edition() {
        fn has_cjk(s: &str) -> bool {
            s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        }
        for lang in ["en", "zh"] {
            let criteria = error_criteria(lang);
            assert_eq!(criteria.len(), 28, "28 类全集: {lang}");
            for tag in ErrorTag::all() {
                let desc = criteria
                    .get(tag.as_str())
                    .unwrap_or_else(|| panic!("缺 criteria {lang}: {}", tag.as_str()));
                assert!(!desc.is_empty(), "描述非空: {lang}/{}", tag.as_str());
                if lang == "en" {
                    assert!(!has_cjk(desc), "en 版全英文（l1 永不进 Jev 请求）: {desc}");
                } else {
                    assert!(has_cjk(desc), "zh 版中文正文: {desc}");
                }
            }
            assert!(criteria.get("mystery").is_none(), "无野键: {lang}");
            assert!(
                std::ptr::eq(criteria, error_criteria(lang)),
                "OnceLock 单例: {lang}"
            );
        }
        assert_eq!(
            error_criteria("ja"),
            error_criteria("en"),
            "缺版回退 en 语料"
        );
    }

    #[test]
    fn test_error_cause_question_encodes_choice_with_criteria() {
        for lang in ["en", "zh"] {
            match error_cause_question(lang) {
                JevQuestion::Choice(q) => {
                    assert_eq!(q.criteria.len(), 28, "criteria 随包: {lang}");
                    assert!(q.instructions.contains("no_error"), "指令含兜底键: {lang}");
                }
                other => panic!("应为 choice: {other:?}"),
            }
        }
        match error_cause_question("ja") {
            JevQuestion::Choice(q) => assert!(
                q.instructions.starts_with("Classify"),
                "未知语种缺版回退 en 指令"
            ),
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
        let questions = error_cause_questions("en", vec![extra]);
        assert!(questions.contains_key("error_cause"), "error_cause 恒在");
        assert!(questions.contains_key("acceptable"), "附加问题并入");
        assert_eq!(questions.len(), 2);
        let bare = error_cause_questions("en", vec![]);
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
