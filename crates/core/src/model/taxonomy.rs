//! 判分词表与枚举序列化口径：Provenance/ErrorTag/Verdict/GradedBy/AttemptMode（as_str 全聚一文件——钩子按名配对）

/// 题目出处（seed 内置 / LLM 生成 / qpack 导入 / 判题沉淀，docs/06-generation §7）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// 内置 seed 题库
    #[default]
    Seed,
    /// LLM 结构化出题（06）
    Generated,
    /// qpack 导入（他人分享）
    Imported,
    /// 判题/讲解自动沉淀
    Collected,
}

impl Provenance {
    /// 持久化字符串（snake_case，与 serde 同口径 —— 漂移由 test_as_str 匹配 serde 断言）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Seed => "seed",
            Self::Generated => "generated",
            Self::Imported => "imported",
            Self::Collected => "collected",
        }
    }
}

/// 错因标签（画像原料，docs/02 §1 全量分类 / docs/07 §3 归类；M3 全量落地）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorTag {
    // 词汇
    WordSense,
    Confusable,
    Collocation,
    Preposition,
    Spelling,
    PosMix,
    Article,
    Countability,
    // 语法
    Tense,
    Voice,
    Sva,
    ClauseLinker,
    NonFinite,
    Subjunctive,
    InversionEmphasis,
    SentenceStructure,
    // 语篇
    Cohesion,
    Reference,
    Logic,
    // 表达（中式英语负迁移主归口）
    Chinglish,
    Register,
    Redundancy,
    PatternMisuse,
    // 应试
    Misread,
    TimePressure,
    Careless,
    // 兜底
    NoError,
    Other,
}

impl ErrorTag {
    /// 稳定字符串（snake_case，入库与 Jev criteria 共用口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WordSense => "word_sense",
            Self::Confusable => "confusable",
            Self::Collocation => "collocation",
            Self::Preposition => "preposition",
            Self::Spelling => "spelling",
            Self::PosMix => "pos_mix",
            Self::Article => "article",
            Self::Countability => "countability",
            Self::Tense => "tense",
            Self::Voice => "voice",
            Self::Sva => "sva",
            Self::ClauseLinker => "clause_linker",
            Self::NonFinite => "non_finite",
            Self::Subjunctive => "subjunctive",
            Self::InversionEmphasis => "inversion_emphasis",
            Self::SentenceStructure => "sentence_structure",
            Self::Cohesion => "cohesion",
            Self::Reference => "reference",
            Self::Logic => "logic",
            Self::Chinglish => "chinglish",
            Self::Register => "register",
            Self::Redundancy => "redundancy",
            Self::PatternMisuse => "pattern_misuse",
            Self::Misread => "misread",
            Self::TimePressure => "time_pressure",
            Self::Careless => "careless",
            Self::NoError => "no_error",
            Self::Other => "other",
        }
    }

    /// 稳定字符串反解（Jev `error_cause` choice → 标签；未知 key → None，调用方 warn 跳过）
    pub fn parse_tag(raw: &str) -> Option<Self> {
        let tag = match raw {
            "word_sense" => Self::WordSense,
            "confusable" => Self::Confusable,
            "collocation" => Self::Collocation,
            "preposition" => Self::Preposition,
            "spelling" => Self::Spelling,
            "pos_mix" => Self::PosMix,
            "article" => Self::Article,
            "countability" => Self::Countability,
            "tense" => Self::Tense,
            "voice" => Self::Voice,
            "sva" => Self::Sva,
            "clause_linker" => Self::ClauseLinker,
            "non_finite" => Self::NonFinite,
            "subjunctive" => Self::Subjunctive,
            "inversion_emphasis" => Self::InversionEmphasis,
            "sentence_structure" => Self::SentenceStructure,
            "cohesion" => Self::Cohesion,
            "reference" => Self::Reference,
            "logic" => Self::Logic,
            "chinglish" => Self::Chinglish,
            "register" => Self::Register,
            "redundancy" => Self::Redundancy,
            "pattern_misuse" => Self::PatternMisuse,
            "misread" => Self::Misread,
            "time_pressure" => Self::TimePressure,
            "careless" => Self::Careless,
            "no_error" => Self::NoError,
            "other" => Self::Other,
            _ => return None,
        };
        Some(tag)
    }

    /// 中文展示名（TUI/报告层用；不参与判分口径，docs/07 §3）
    pub fn label_zh(&self) -> &'static str {
        match self {
            Self::WordSense => "词义",
            Self::Confusable => "易混词",
            Self::Collocation => "搭配",
            Self::Preposition => "介词",
            Self::Spelling => "拼写",
            Self::PosMix => "词性",
            Self::Article => "冠词",
            Self::Countability => "可数性",
            Self::Tense => "时态",
            Self::Voice => "语态",
            Self::Sva => "主谓一致",
            Self::ClauseLinker => "从句连词",
            Self::NonFinite => "非谓语",
            Self::Subjunctive => "虚拟语气",
            Self::InversionEmphasis => "倒装强调",
            Self::SentenceStructure => "句子结构",
            Self::Cohesion => "衔接",
            Self::Reference => "指代",
            Self::Logic => "逻辑",
            Self::Chinglish => "中式英语",
            Self::Register => "语域",
            Self::Redundancy => "冗余",
            Self::PatternMisuse => "句式误用",
            Self::Misread => "审题偏差",
            Self::TimePressure => "时间压力",
            Self::Careless => "粗心",
            Self::NoError => "无错",
            Self::Other => "其它",
        }
    }

    /// 全量分类（表驱动测试与语料校验用：顺序稳定）
    pub fn all() -> &'static [ErrorTag] {
        &[
            Self::WordSense,
            Self::Confusable,
            Self::Collocation,
            Self::Preposition,
            Self::Spelling,
            Self::PosMix,
            Self::Article,
            Self::Countability,
            Self::Tense,
            Self::Voice,
            Self::Sva,
            Self::ClauseLinker,
            Self::NonFinite,
            Self::Subjunctive,
            Self::InversionEmphasis,
            Self::SentenceStructure,
            Self::Cohesion,
            Self::Reference,
            Self::Logic,
            Self::Chinglish,
            Self::Register,
            Self::Redundancy,
            Self::PatternMisuse,
            Self::Misread,
            Self::TimePressure,
            Self::Careless,
            Self::NoError,
            Self::Other,
        ]
    }
}

/// 判定结果（Pending = 待裁定桶，docs/07-grading）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Correct,
    Partial,
    Wrong,
    Acceptable,
    Pending,
}

impl Verdict {
    /// 持久化字符串（snake_case，与 serde 同口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Correct => "correct",
            Self::Partial => "partial",
            Self::Wrong => "wrong",
            Self::Acceptable => "acceptable",
            Self::Pending => "pending",
        }
    }
}

/// 判分来源（M1 只有 Rule；Jev/LLM 为 M3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradedBy {
    Rule,
    Jev,
    LlmReview,
    Human,
}

impl GradedBy {
    /// 持久化字符串（snake_case，与 serde 同口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Jev => "jev",
            Self::LlmReview => "llm_review",
            Self::Human => "human",
        }
    }
}

/// 作答场景
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptMode {
    Practice,
    Review,
    Mock,
    Game,
    Typing,
}

impl AttemptMode {
    /// 持久化字符串（snake_case，与 serde 同口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Practice => "practice",
            Self::Review => "review",
            Self::Mock => "mock",
            Self::Game => "game",
            Self::Typing => "typing",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::*;
    use crate::{Answer, Question, QuestionKind};

    #[test]
    fn test_as_str_is_snake_case() {
        assert_eq!(ErrorTag::WordSense.as_str(), "word_sense");
        assert_eq!(ErrorTag::NoError.as_str(), "no_error");
        assert_eq!(ErrorTag::Careless.as_str(), "careless");
        assert_eq!(ErrorTag::NonFinite.as_str(), "non_finite");
        assert_eq!(ErrorTag::PosMix.as_str(), "pos_mix");
        assert_eq!(ErrorTag::InversionEmphasis.as_str(), "inversion_emphasis");
    }

    #[test]
    fn test_parse_tag_roundtrips_every_variant() {
        let all = ErrorTag::all();
        assert_eq!(all.len(), 28, "docs/02 §1 全量 28 类");
        for tag in all {
            let raw = tag.as_str();
            assert_eq!(ErrorTag::parse_tag(raw), Some(*tag), "往返一致: {raw}");
            assert_eq!(
                serde_json::to_value(*tag).expect("序列化"),
                serde_json::json!(raw),
                "serde 与 as_str 同口径: {raw}"
            );
        }
        assert_eq!(ErrorTag::parse_tag("mystery"), None, "未知 key → None");
        // 去重：28 个稳定字符串互不相同
        let mut keys: Vec<&str> = all.iter().map(|t| t.as_str()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "稳定字符串无重复");
    }

    #[test]
    fn test_label_zh_covers_all_variants_translated() {
        let mut labels: Vec<&str> = ErrorTag::all().iter().map(|t| t.label_zh()).collect();
        assert_eq!(labels.len(), 28, "28 类全有中文名");
        for label in &labels {
            assert!(!label.is_empty(), "中文名非空");
            assert!(
                label
                    .chars()
                    .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                "是中文: {label}"
            );
        }
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        labels.clear();
        assert_eq!(sorted.len(), 28, "中文名互不重复（UI 不混淆）");
        assert_eq!(ErrorTag::Chinglish.label_zh(), "中式英语");
    }

    #[test]
    fn test_all_lists_28_variants() {
        let all = ErrorTag::all();
        assert_eq!(all.len(), 28, "docs/02 §1 全量 28 类");
        assert_eq!(*all.first().expect("非空"), ErrorTag::WordSense, "顺序稳定");
        assert_eq!(*all.last().expect("非空"), ErrorTag::Other, "兑底收尾");
    }

    #[test]
    fn test_provenance_as_str_matches_serde_snake_case() {
        for p in [
            Provenance::Seed,
            Provenance::Generated,
            Provenance::Imported,
            Provenance::Collected,
        ] {
            let serde_str = serde_json::to_value(p).expect("序列化").to_string();
            assert_eq!(
                serde_str,
                format!("\"{}\"", p.as_str()),
                "as_str 与 serde 同口径"
            );
        }
    }

    #[test]
    fn test_persist_enums_as_str_match_serde_snake_case() {
        for v in [
            Verdict::Correct,
            Verdict::Partial,
            Verdict::Wrong,
            Verdict::Acceptable,
            Verdict::Pending,
        ] {
            assert_eq!(
                serde_json::to_value(v).expect("序列化").to_string(),
                format!("\"{}\"", v.as_str())
            );
        }
        for g in [GradedBy::Rule, GradedBy::LlmReview, GradedBy::Human] {
            assert_eq!(
                serde_json::to_value(g).expect("序列化").to_string(),
                format!("\"{}\"", g.as_str())
            );
        }
        for m in [AttemptMode::Practice, AttemptMode::Typing] {
            assert_eq!(
                serde_json::to_value(m).expect("序列化").to_string(),
                format!("\"{}\"", m.as_str())
            );
        }
        assert_eq!(GradedBy::LlmReview.as_str(), "llm_review");
    }

    #[test]
    fn test_enums_serialize_snake_case_in_json() {
        let q = Question::new(
            "choose".into(),
            QuestionKind::SingleChoice(test_choice_body()),
            1,
            vec![],
            Provenance::Seed,
        );
        let json = serde_json::to_string(&q.kind).expect("序列化题型");
        assert!(
            json.starts_with("{\"single_choice\""),
            "题型枚举 snake_case: {json}"
        );
        let ans = serde_json::to_string(&Answer::Choice(vec![0])).expect("序列化作答");
        assert!(ans.contains("choice"), "作答枚举 snake_case: {ans}");
    }
}
