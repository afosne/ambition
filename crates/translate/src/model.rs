//! 工作台类型（docs/11 §1 契约；Translation 行类型在 core——store 落库共用，Register 在 core §1）
use ambition_core::{GradeResult, LangTag, Register, Translation};

/// 翻译参数（docs/11 §1：语种对 + 语域 + 逐词优先 + 术语表）
#[derive(Debug, Clone, PartialEq)]
pub struct TranslateOpts {
    pub l1: LangTag,
    pub l2: LangTag,
    pub register: Register,
    pub literal_first: bool,
    pub glossary: Vec<(String, String)>,
}

impl TranslateOpts {
    /// 构造（缺省：语域 Neutral、非逐词优先、无术语表——M5 工作台默认）
    pub fn new(l1: LangTag, l2: LangTag) -> Self {
        Self {
            l1,
            l2,
            register: Register::Neutral,
            literal_first: false,
            glossary: Vec::new(),
        }
    }

    /// 术语命中数（source 含源语词 或 target 含目标语词即计——observability 指标、非判分）
    pub fn glossary_hits(&self, source: &str, target: &str) -> u32 {
        self.glossary
            .iter()
            .filter(|(a, b)| source.contains(a.as_str()) || target.contains(b.as_str()))
            .count() as u32
    }
}

/// 对齐切段类别（literal/idiomatic/reordered/omitted，docs/11 §1；serde snake_case = LLM wire）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignKind {
    Literal,
    Idiomatic,
    Reordered,
    Omitted,
}

/// 对齐切段（源/译/类别——LLM 结构化直读）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlignSeg {
    pub src: String,
    pub tgt: String,
    pub kind: AlignKind,
}

/// 句/短语级对齐结果（align 出口，docs/11 §2）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Alignment {
    pub segments: Vec<AlignSeg>,
}

/// 差异点类别（Same/Changed/Missing/Extra——word/字级 LCS，docs/11 §4）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Same,
    Changed,
    Missing,
    Extra,
}

/// 差异点（my_turn 对照展示用；非持久化）
#[derive(Debug, Clone, PartialEq)]
pub struct DiffPoint {
    pub src: String,
    pub tgt: String,
    pub kind: DiffKind,
}

/// 我来翻结果（参考译文 + 差异 + 评分；cards 字段随 08-adaptive——docs/11 §1 M5 注记）
#[derive(Debug, Clone, PartialEq)]
pub struct CompareResult {
    pub reference: Translation,
    pub diff: Vec<DiffPoint>,
    pub grade: Option<GradeResult>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::LangTag;

    #[test]
    fn test_new_translate_opts_defaults() {
        let opts = TranslateOpts::new(LangTag("zh-Hans".into()), LangTag::english());
        assert_eq!(opts.register, Register::Neutral, "缺省语域");
        assert!(!opts.literal_first, "缺省非逐词优先");
        assert!(opts.glossary.is_empty(), "缺省无术语表");
    }

    #[test]
    fn test_glossary_hits_counts_either_side_match() {
        let mut opts = TranslateOpts::new(LangTag("zh-Hans".into()), LangTag::english());
        opts.glossary
            .push(("剪纸".to_string(), "paper-cutting".to_string()));
        opts.glossary
            .push(("灯笼".to_string(), "lantern".to_string()));
        assert_eq!(opts.glossary_hits("剪纸艺术", "art"), 1, "源语侧命中");
        assert_eq!(
            opts.glossary_hits("art", "paper-cutting"),
            1,
            "目标语侧命中"
        );
        assert_eq!(opts.glossary_hits("art", "painting"), 0, "未命中为 0");
    }
}
