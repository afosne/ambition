//! 批注闭环类型与提示词（M5-W5，docs/10 §2）：Comments 结构 + comments_schema + annotate 提示词（按 l2 选版）
//! LLM 通道在 engine（LlmKind::Explain + writing 资产 prompts/annotate）；缓存 = llm run_loop effective_cache_key 自动口径。
//! 本域子组件：`annotate::upgrade`（升级候选/同义表）· `annotate::recommend`（补练推荐/短语库）· `annotate::model_essay`（三档范文）。

mod model_essay;
mod recommend;
mod upgrade;

pub use model_essay::*;
pub use recommend::*;
pub use upgrade::*;

use ambition_core::{GradeResult, LangTag, pick_edition};

/// annotate 系统指令（短指令在码、模板在资产——同 gen 先例，docs/05 §1）
const ANNOTATE_INSTRUCTION: &str = "You are an experienced English writing tutor. Judge every sentence of the essay and reply with the required JSON only.";

/// 自由作文提交（M5-W5，docs/10 §2：grade_essay 输入——一次提交带全上下文，参数>5 组 struct，docs/01）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EssaySubmission {
    pub body: String,
    pub prompt_zh: String,
    pub genre: String,
    pub rubric_id: String,
    pub scaffold_stage: String,
    pub template_id: String,
}

/// 批注类别（升级 / 亮点）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentKind {
    Upgrade,
    Highlight,
}

/// SRS 可学目标类型（升级句里可入卡的词/短语/句式）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WordKind {
    Word,
    Phrase,
    Expression,
}

/// 升级句可入卡目标（Phrase 卡随 08-adaptive 落位，M5 先随批注透传）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SrsTarget {
    pub kind: WordKind,
    pub text: String,
}

/// 单句批注（annotate 出口单元）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Annotation {
    /// 原句（逐字保留，TUI 对照定位）
    pub sentence: String,
    pub kind: CommentKind,
    /// 升级建议句（kind=Upgrade 时给出；Highlight 为空）
    #[serde(default)]
    pub suggested: Option<String>,
    pub brief_reason: String,
    /// 可入卡目标（可省略）
    #[serde(default)]
    pub target: Option<SrsTarget>,
}

/// 逐句批注集合（= LLM structured wire，docs/10 §2）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Comments {
    pub annotations: Vec<Annotation>,
}

/// 加分项提示词（模板资产按 l2 选版；占位 {{essay}}/{{rubric_id}}/{{band}}/{{max}}）
/// 返回（系统指令, 用户提示词）
pub fn annotate_prompt(
    l2: &LangTag,
    essay: &str,
    rubric_id: &str,
    grade: &GradeResult,
) -> (String, String) {
    let (template, edition) = pick_edition(
        &l2.0,
        include_str!("../../assets/prompts/annotate/zh.md"),
        include_str!("../../assets/prompts/annotate/en.md"),
    );
    let prompt = template
        .replace("{{l2}}", &l2.name_in(edition))
        .replace("{{essay}}", essay)
        .replace("{{rubric_id}}", rubric_id)
        .replace("{{band}}", &grade.score.to_string())
        .replace("{{max}}", &grade.max.to_string());
    (ANNOTATE_INSTRUCTION.to_string(), prompt)
}

/// annotate 结构化输出 schema（llm response_format；字段名与 Comments serde 同名）
pub fn comments_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["annotations"],
        "properties": {
            "annotations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["sentence", "kind", "brief_reason"],
                    "properties": {
                        "sentence": { "type": "string" },
                        "kind": { "type": "string", "enum": ["upgrade", "highlight"] },
                        "suggested": { "type": "string" },
                        "brief_reason": { "type": "string" },
                        "target": {
                            "type": "object",
                            "required": ["kind", "text"],
                            "properties": {
                                "kind": { "type": "string", "enum": ["word", "phrase", "expression"] },
                                "text": { "type": "string" }
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

    fn grade() -> GradeResult {
        GradeResult {
            verdict: ambition_core::Verdict::Correct,
            score: 20.0,
            max: 25.0,
            confidence: 0.9,
            graded_by: ambition_core::GradedBy::Jev,
            detail: ambition_core::GradeDetail::Plain,
            error_tags: vec![],
        }
    }

    #[test]
    fn test_annotate_prompt_fills_placeholders_and_uses_edition() {
        let (instruction, prompt) = annotate_prompt(
            &LangTag::english(),
            "Nowadays I think this is good.",
            "gaokao25",
            &grade(),
        );
        assert!(instruction.contains("JSON"), "系统指令在场");
        assert!(
            prompt.contains("Nowadays I think this is good."),
            "essay 注入"
        );
        assert!(prompt.contains("gaokao25"), "rubric 注入");
        assert!(prompt.contains("band score 20/25"), "档位注入: {prompt}");
        assert!(!prompt.contains("{{"), "占位全部替换: {prompt}");
        assert!(
            !prompt.contains("(en)") && prompt.contains("feedback (English)")
                || !prompt.contains("{{"),
            "l2 名称注入"
        );

        let (_, zh) = annotate_prompt(&LangTag("zh-Hans".into()), "essay", "g", &grade());
        assert!(
            zh.contains("批改对象") || zh.contains("作文"),
            "zh 选版: {zh}"
        );
    }

    #[test]
    fn test_comments_schema_matches_comments_serde_shape() {
        let schema = comments_schema();
        assert_eq!(schema["required"][0], "annotations");
        let kinds = schema["properties"]["annotations"]["items"]["properties"]["kind"]["enum"]
            .as_array()
            .expect("kind 枚举");
        assert_eq!(kinds.len(), 2, "upgrade/highlight 两值");
        // serde 反序列化同名字段（wire 一致性）
        let wire = r#"{"annotations":[{"sentence":"S.","kind":"upgrade","suggested":"B.","brief_reason":"stuffy","target":{"kind":"phrase","text":"take into account"}}]}"#;
        let parsed: Comments = serde_json::from_str(wire).expect("wire 解析");
        assert_eq!(parsed.annotations.len(), 1);
        assert_eq!(parsed.annotations[0].kind, CommentKind::Upgrade);
        assert_eq!(
            parsed.annotations[0].target.as_ref().map(|t| t.kind),
            Some(WordKind::Phrase)
        );
    }
}
