//! prompt 渲染与 JSON Schema：render_prompt/load_prompt/draft_schema（模板按 l2 分版、编译期内嵌）

use super::*;

/// prompt 模板渲染（模板按 l2 分版、缺版回退 en；{{l1}}/{{l2}} 占位替换为语言名，docs/05 §1）
pub(crate) fn render_prompt(spec: &GenSpec, needed: u32, failures: &[String]) -> String {
    let (template, name_lang) = load_prompt(&spec.l2);
    let kinds = spec
        .kinds
        .iter()
        .map(QuestionKindTag::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let topic_line = match &spec.topic {
        Some(topic) => format!("- Topic focus: {topic}.\n"),
        None => String::new(),
    };
    let feedback_line = if failures.is_empty() {
        String::new()
    } else {
        format!(
            "\nPrevious attempts were rejected:\n{}\nFix these issues.",
            failures
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    // {{l1}}/{{l2}} → 语言名（用模板自身语言书写；l1 现取常量，TODO(16-config)：随 config 注入）
    let l1 = ambition_core::LangTag(ambition_core::DEFAULT_L1.into());
    let mut out = template
        .replace("{{count}}", &needed.to_string())
        .replace("{{kinds}}", &kinds)
        .replace("{{track}}", &spec.track.0)
        .replace(
            "{{difficulty}}",
            &format!("{}-{}", spec.difficulty.0, spec.difficulty.1),
        )
        .replace("{{l1}}", &l1.name_in(name_lang))
        .replace("{{l2}}", &spec.l2.name_in(name_lang))
        .replace("{{topic_line}}", &topic_line)
        .replace("{{feedback_line}}", &feedback_line);
    if let Some(words) = &spec.source_words {
        out.push_str(&format!(
            "\nUse these target words where natural: {}.\n",
            words.0.join(", ")
        ));
    }
    if let Some(seed) = &spec.seed_qid {
        out.push_str(&format!(
            "\nCreate a new question on the same knowledge point as seed {}.\n",
            seed.0
        ));
    }
    if !spec.weak_points.is_empty() {
        let tags = spec
            .weak_points
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("\nTarget learner weak points: {tags}.\n"));
    }
    out
}

/// prompt 模板选择：按 l2 分版、缺版回退 en（返回「模板原文、模板书写语言」；编译期内嵌不读运行时文件，docs/05 §1）
pub(crate) fn load_prompt(l2: &ambition_core::LangTag) -> (&'static str, &'static str) {
    // 选版口径收口到 core::pick_edition（zh 谱系→zh 版，其余缺版回退 en，docs/05 §1）
    ambition_core::pick_edition(
        &l2.0,
        include_str!("../../assets/prompts/question_gen/zh.md"),
        include_str!("../../assets/prompts/question_gen/en.md"),
    )
}

/// 草稿 JSON Schema（llm response_format 约束）
pub(crate) fn draft_schema() -> JsonSchema {
    serde_json::json!({
        "type": "object",
        "required": ["questions"],
        "properties": {
            "questions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["kind", "stem", "answers", "difficulty", "tags"],
                    "properties": {
                        "kind": {"type": "string", "enum": ["single_choice", "multi_choice", "fill_blank", "cloze", "reading", "seven_of_five", "grammar_fill"]},
                        "stem": {"type": "string"},
                        "options": {"type": "array", "items": {"type": "string"}},
                        "answers": {"type": "array", "items": {"type": "string"}},
                        "blanks": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}},
                        "passage": {"type": "string"},
                        "items": {"type": "array", "items": {"type": "object", "required": ["question", "options", "answers"], "properties": {"question": {"type": "string"}, "options": {"type": "array", "items": {"type": "string"}}, "answers": {"type": "array", "items": {"type": "string"}}}}},
                        "difficulty": {"type": "integer", "minimum": 1, "maximum": 5},
                        "tags": {"type": "array", "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;

    #[test]
    fn test_render_prompt_substitutes_placeholders_and_feedback() {
        let mut s = spec();
        s.topic = Some("campus life".into());
        s.source_words = Some(WordSet(vec!["dormitory".into(), "tuition".into()]));
        s.weak_points = vec![ambition_core::ErrorTag::Tense];
        s.seed_qid = Some(ambition_core::Qid("deadbeef".into()));
        let prompt = render_prompt(&s, 2, &["qc failed: stem ambiguous".into()]);
        assert!(!prompt.contains("{{count}}"), "占位全部替换: {prompt}");
        assert!(prompt.contains("exactly 2 single_choice, fill_blank"));
        assert!(prompt.contains("cet4"));
        assert!(prompt.contains("1-3"));
        assert!(prompt.contains("campus life"));
        assert!(prompt.contains("Previous attempts were rejected"));
        assert!(prompt.contains("dormitory"));
        assert!(prompt.contains("deadbeef"));
        assert!(prompt.contains("tense"));
        assert!(prompt.contains("English"), "{{l2}} → 语言名");
        assert!(prompt.contains("Chinese"), "{{l1}} → 语言名（模板语言 en）");

        let clean = render_prompt(
            &GenSpec::new("gaokao", vec![QuestionKindTag::FillBlank], 1),
            1,
            &[],
        );
        assert!(!clean.contains("Previous attempts"), "无失败则无反馈段");
        assert!(!clean.contains("{{"));
    }

    #[test]
    fn test_load_prompt_returns_embedded_template() {
        let (text, lang) = load_prompt(&ambition_core::LangTag::english());
        assert!(text.contains("{{count}}"), "模板占位在场");
        assert!(text.contains("{{l2}}"), "模板只带占位，语言名渲染时注入");
        assert_eq!(lang, "en");
    }

    #[test]
    fn test_load_prompt_picks_l2_edition_with_en_fallback() {
        let (zh, zh_lang) = load_prompt(&ambition_core::LangTag("zh".into()));
        assert!(zh.contains("命题"), "zh 版中文正文");
        assert_eq!(zh_lang, "zh");
        let (miss, miss_lang) = load_prompt(&ambition_core::LangTag("ja".into()));
        assert!(miss.contains("Question generation"), "缺版回退 en");
        assert_eq!(miss_lang, "en");
    }

    #[test]
    fn test_render_prompt_uses_l2_edition_and_name_language() {
        let mut zh = GenSpec::new("zh_track", vec![QuestionKindTag::FillBlank], 1);
        zh.l2 = ambition_core::LangTag("zh".into());
        let zh_prompt = render_prompt(&zh, 1, &[]);
        assert!(zh_prompt.contains("命题"), "zh 版正文");
        assert!(
            zh_prompt.contains("中文"),
            "zh 模板里 {{l2}}/{{l1}} 用中文名"
        );
        assert!(!zh_prompt.contains("{{"), "占位全部替换");

        let mut ja = GenSpec::new("jlpt", vec![QuestionKindTag::FillBlank], 1);
        ja.l2 = ambition_core::LangTag("ja".into());
        let ja_prompt = render_prompt(&ja, 1, &[]);
        assert!(
            ja_prompt.contains("Question generation"),
            "缺版回退 en 正文"
        );
        assert!(ja_prompt.contains("Japanese"), "en 模板用英文名写实际 l2");
        assert!(ja_prompt.contains("Chinese"), "{{l1}} 仍为母语名");
        assert!(!ja_prompt.contains("{{"), "占位全部替换");
    }

    #[test]
    fn test_draft_schema_requires_core_fields() {
        let schema = draft_schema();
        let required = schema["properties"]["questions"]["items"]["required"]
            .as_array()
            .expect("required 数组");
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"kind"));
        assert!(names.contains(&"stem"));
        assert!(names.contains(&"answers"));
        assert_eq!(
            schema["properties"]["questions"]["items"]["properties"]["kind"]["enum"][0],
            "single_choice"
        );
    }
}
