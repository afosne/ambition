//! 三档范文 + 逐句赏析（M5-W5，docs/10 §2 model_essays；200/400/600 词、qc.natural 口径随 §4 论证库波次）

use ambition_core::pick_edition;

/// 三档范文系统指令（短指令在码、模板在资产——同 gen 先例）
const MODEL_ESSAY_INSTRUCTION: &str = "You are an exam essay coach for an English learning app. Follow the tier word counts strictly and reply with the required JSON only.";

/// 单句赏析（逐字引用 + 中文讲评）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SentenceNote {
    pub sentence: String,
    pub note_zh: String,
}

/// 单档范文（word_target = 200|400|600）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelEssay {
    pub word_target: u32,
    pub text: String,
    pub sentences: Vec<SentenceNote>,
}

/// 三档范文集（annotate/model_essays 出口；= LLM structured wire）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelEssaySet {
    pub essays: Vec<ModelEssay>,
}

/// 范文提示词（模板资产按 l2 选版；占位 {{prompt}}）
/// 返回（系统指令, 用户提示词）
pub fn model_essay_prompt(l2: &ambition_core::LangTag, prompt: &str) -> (String, String) {
    let (template, edition) = pick_edition(
        &l2.0,
        include_str!("../../assets/prompts/model_essays/zh.md"),
        include_str!("../../assets/prompts/model_essays/en.md"),
    );
    (
        MODEL_ESSAY_INSTRUCTION.to_string(),
        template
            .replace("{{l2}}", &l2.name_in(edition))
            .replace("{{prompt}}", prompt),
    )
}

/// 三档范文结构化 schema（llm response_format；字段名与 ModelEssaySet serde 同名）
pub fn model_essay_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["essays"],
        "properties": {
            "essays": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["word_target", "text", "sentences"],
                    "properties": {
                        "word_target": { "type": "integer" },
                        "text": { "type": "string" },
                        "sentences": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["sentence", "note_zh"],
                                "properties": {
                                    "sentence": { "type": "string" },
                                    "note_zh": { "type": "string" }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::LangTag;

    #[test]
    fn test_model_essay_prompt_injects_prompt_and_keeps_tiers_doc() {
        let (instruction, prompt) =
            model_essay_prompt(&LangTag::english(), "Should phones be banned on campus?");
        assert!(instruction.contains("JSON"), "系统指令在场");
        assert!(
            prompt.contains("Should phones be banned on campus?"),
            "题目注入: {prompt}"
        );
        assert!(prompt.contains("200, 400 and 600"), "三档口径在模板中");
        assert!(!prompt.contains("{{"), "占位全部替换: {prompt}");

        let (_, zh) = model_essay_prompt(&LangTag("zh-Hans".into()), "题目");
        assert!(zh.contains("三篇"), "zh 选版: {zh}");
    }

    #[test]
    fn test_model_essay_schema_matches_serde_shape() {
        let schema = model_essay_schema();
        assert_eq!(schema["required"][0], "essays");
        let wire = r#"{"essays":[{"word_target":400,"text":"essay body","sentences":[{"sentence":"S.","note_zh":"好在衔接"}]}]}"#;
        let parsed: ModelEssaySet = serde_json::from_str(wire).expect("wire 解析");
        assert_eq!(parsed.essays[0].word_target, 400);
        assert_eq!(parsed.essays[0].sentences[0].note_zh, "好在衔接");
    }
}
