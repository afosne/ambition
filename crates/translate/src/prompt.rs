//! 提示词与出译 schema（M5-W7，docs/11 §2；模板资产 include_str、l2 选版走 core::pick_edition）
//! 短指令在码、模板在资产（同 gen 先例——指令行不进 data/，红线 #19 只管资产侧）
use ambition_core::{Register, pick_edition};

use crate::model::TranslateOpts;

/// 出译系统指令（LLM-facing，短句在码）
const TRANSLATE_INSTRUCTION: &str =
    "You are a professional translator. Return the required JSON object only.";
/// 对照对齐系统指令
const ALIGN_INSTRUCTION: &str =
    "You are a meticulous bilingual aligner. Return the required JSON object only.";

/// translate 模板（按目标语 l2 选版；缺版回退 en——core::pick_edition 单口径）
fn load_translate(l2: &str) -> &'static str {
    pick_edition(
        l2,
        include_str!("../assets/prompts/translate/zh.md"),
        include_str!("../assets/prompts/translate/en.md"),
    )
    .0
}

/// align 模板（同上按 l2 选版）
fn load_align(l2: &str) -> &'static str {
    pick_edition(
        l2,
        include_str!("../assets/prompts/align/zh.md"),
        include_str!("../assets/prompts/align/en.md"),
    )
    .0
}

/// 翻译提示词（{{text}}/{{glossary_block}}/{{register}}/{{literal_note}} 占位；术语表强制一致）
pub fn render_translate_prompt(opts: &TranslateOpts, text: &str) -> (String, String) {
    let template = load_translate(&opts.l2.0);
    let glossary_block = if opts.glossary.is_empty() {
        "(none)".to_string()
    } else {
        opts.glossary
            .iter()
            .map(|(a, b)| format!("- {a} → {b}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let register = match opts.register {
        Register::Neutral => "neutral",
        Register::Formal => "formal",
        Register::Casual => "casual",
    };
    let literal_note = if opts.literal_first {
        "Draft a literal version first, then refine it."
    } else {
        "Translate directly."
    };
    let prompt = template
        .replace("{{text}}", text)
        .replace("{{glossary_block}}", &glossary_block)
        .replace("{{register}}", register)
        .replace("{{literal_note}}", literal_note);
    (TRANSLATE_INSTRUCTION.to_string(), prompt)
}

/// 对照对齐提示词（{{source}}/{{target}} 占位；只切段不改写）
pub fn render_align_prompt(opts: &TranslateOpts, source: &str, target: &str) -> (String, String) {
    let template = load_align(&opts.l2.0);
    let prompt = template
        .replace("{{source}}", source)
        .replace("{{target}}", target);
    (ALIGN_INSTRUCTION.to_string(), prompt)
}

/// 出译 structured 响应（run_loop structured<T> 直读；缓存按 effective_cache_key 自动命中）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TranslateWire {
    pub target: String,
}

/// 出译 schema（provider response_format；极简单字段）
pub fn translate_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["target"],
        "properties": { "target": { "type": "string" } }
    })
}

/// 对齐 schema（segments 切段数组；kind 枚举与 AlignKind snake_case 对齐）
pub fn align_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["segments"],
        "properties": {
            "segments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["src", "tgt", "kind"],
                    "properties": {
                        "src": { "type": "string" },
                        "tgt": { "type": "string" },
                        "kind": { "type": "string", "enum": ["literal", "idiomatic", "reordered", "omitted"] }
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

    fn opts() -> TranslateOpts {
        let mut o = TranslateOpts::new(LangTag("zh-Hans".into()), LangTag::english());
        o.glossary
            .push(("剪纸".to_string(), "paper-cutting".to_string()));
        o
    }

    #[test]
    fn test_render_translate_prompt_injects_text_and_glossary() {
        let (instruction, prompt) = render_translate_prompt(&opts(), "我喜欢春天。");
        assert!(
            instruction.contains("professional translator"),
            "短指令在码"
        );
        assert!(prompt.contains("我喜欢春天。"), "源文入模板");
        assert!(prompt.contains("剪纸 → paper-cutting"), "术语表强制一致");
        assert!(prompt.contains("neutral"), "语域注入");
        assert!(!prompt.contains("{{"), "占位全替换: {prompt}");
    }

    #[test]
    fn test_render_translate_prompt_falls_back_to_en_template_for_en_target() {
        let (_, prompt) = render_translate_prompt(&opts(), "hello");
        assert!(
            !prompt.contains("翻译任务"),
            "l2=en 走 en 模板（pick_edition 单口径）"
        );
        assert!(prompt.contains("Translation task"), "en 模板标题在场");
    }

    #[test]
    fn test_render_align_prompt_injects_both_sides() {
        let (_, prompt) = render_align_prompt(&opts(), "我喜欢春天。", "I like spring.");
        assert!(prompt.contains("我喜欢春天。"), "源句在场");
        assert!(prompt.contains("I like spring."), "译句在场");
        assert!(!prompt.contains("{{"), "占位全替换");
    }

    #[test]
    fn test_load_translate_picks_template_by_target_lang() {
        assert!(
            load_translate("en").contains("Translation task"),
            "en 目标语走 en 模板"
        );
        assert!(
            load_translate("zh-Hans").contains("翻译任务"),
            "zh 谱系走 zh 模板"
        );
    }

    #[test]
    fn test_load_align_picks_template_by_target_lang() {
        assert!(
            load_align("en").contains("Segment alignment"),
            "en 目标语走 en 模板"
        );
        assert!(
            load_align("zh-Hans").contains("分段对齐"),
            "zh 谱系走 zh 模板"
        );
    }

    #[test]
    fn test_translate_schema_requires_target() {
        let schema = translate_schema();
        assert_eq!(schema["required"], serde_json::json!(["target"]));
        assert_eq!(schema["type"], serde_json::json!("object"));
    }

    #[test]
    fn test_align_schema_requires_segments() {
        let schema = align_schema();
        assert_eq!(schema["required"], serde_json::json!(["segments"]));
    }

    #[test]
    fn test_schemas_pin_required_shapes() {
        assert_eq!(
            translate_schema()["required"],
            serde_json::json!(["target"]),
            "出译 schema 单字段"
        );
        let kinds =
            &align_schema()["properties"]["segments"]["items"]["properties"]["kind"]["enum"];
        assert_eq!(
            kinds,
            &serde_json::json!(["literal", "idiomatic", "reordered", "omitted"]),
            "对齐类别枚举与 AlignKind 对齐"
        );
    }
}
