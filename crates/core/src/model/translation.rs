//! 工作台译文记录（M5-W7，docs/11 §1——类型在 core：store 落库与 engine 门面共用）
use super::question::TranslationDirection;

/// 单条译文（translate 结构化出 target；glossary_hits = 术语表命中数、本地统计非 LLM）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Translation {
    pub tx_id: String,
    pub source: String,
    pub target: String,
    pub dir: TranslationDirection,
    pub model: String,
    pub glossary_hits: u32,
}
