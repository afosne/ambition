//! prompt 渲染与 JSON Schema：render_prompt/load_prompt/draft_schema（data/prompts 可覆盖）

use super::*;

/// prompt 模板渲染（data/prompts 可覆盖，内嵌兜底；{{l2}} 等占位 docs/05 §1）
pub(crate) fn render_prompt(spec: &GenSpec, needed: u32, failures: &[String]) -> String {
    let template = load_prompt();
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
    let mut out = template
        .replace("{{count}}", &needed.to_string())
        .replace("{{kinds}}", &kinds)
        .replace("{{track}}", &spec.track.0)
        .replace(
            "{{difficulty}}",
            &format!("{}-{}", spec.difficulty.0, spec.difficulty.1),
        )
        .replace("{{l2}}", "English")
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

/// prompt 模板：优先 data/prompts/question_gen.md（cwd 相对），否则内嵌同名文件
pub(crate) fn load_prompt() -> String {
    std::fs::read_to_string("data/prompts/question_gen.md")
        .unwrap_or_else(|_| include_str!("../../../data/prompts/question_gen.md").to_string())
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
                        "kind": {"type": "string", "enum": ["single_choice", "multi_choice", "fill_blank"]},
                        "stem": {"type": "string"},
                        "options": {"type": "array", "items": {"type": "string"}},
                        "answers": {"type": "array", "items": {"type": "string"}},
                        "blanks": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}},
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

    fn spec() -> GenSpec {
        GenSpec::new(
            "cet4",
            vec![QuestionKindTag::SingleChoice, QuestionKindTag::FillBlank],
            3,
        )
    }

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
        assert!(prompt.contains("English"));

        let clean = render_prompt(
            &GenSpec::new("gaokao", vec![QuestionKindTag::FillBlank], 1),
            1,
            &[],
        );
        assert!(!clean.contains("Previous attempts"), "无失败则无反馈段");
        assert!(!clean.contains("{{"));
    }

    #[test]
    fn test_load_prompt_falls_back_to_embedded_template() {
        let text = load_prompt();
        assert!(text.contains("{{count}}"), "模板占位在场");
        assert!(text.contains("English"), "英文模板");
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
