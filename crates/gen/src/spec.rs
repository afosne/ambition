//! 出题规格与标签：GenSpec/TrackId/QuestionKindTag/WordSet（docs/06 §1）

/// 考试轨（stages/<lang>/<track>，docs/16）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrackId(pub String);

/// 生成题型标签（prompt 与 schema 用的稳定字符串）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKindTag {
    SingleChoice,
    MultiChoice,
    FillBlank,
}

impl QuestionKindTag {
    /// 稳定字符串（prompt 拼接与草稿校验共用）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SingleChoice => "single_choice",
            Self::MultiChoice => "multi_choice",
            Self::FillBlank => "fill_blank",
        }
    }

    /// 是否选择类题型（决定质检是否问 distractor/unique）
    pub fn is_choice(&self) -> bool {
        matches!(self, Self::SingleChoice | Self::MultiChoice)
    }
}

/// 词集（source_words 输入）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WordSet(pub Vec<String>);

/// 生成规格（docs/06 §1；difficulty 必来自 CAT——M2 由调用方给定区间）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenSpec {
    pub track: TrackId,
    pub kinds: Vec<QuestionKindTag>,
    pub count: u32,
    pub difficulty: (u8, u8),
    pub topic: Option<String>,
    pub source_words: Option<WordSet>,
    pub weak_points: Vec<ambition_core::ErrorTag>,
    pub seed_qid: Option<ambition_core::Qid>,
    /// 目标语：prompt 按 l2 分版 + `{{l2}}` 占位，缺版回退 en（docs/05 §1）
    #[serde(default)]
    pub l2: ambition_core::LangTag,
}

impl GenSpec {
    /// 构造基础规格（difficulty/topic 等由调用方补字段）
    pub fn new(track: &str, kinds: Vec<QuestionKindTag>, count: u32) -> Self {
        Self {
            track: TrackId(track.to_string()),
            kinds,
            count,
            difficulty: (1, 3),
            topic: None,
            source_words: None,
            weak_points: Vec::new(),
            seed_qid: None,
            l2: ambition_core::LangTag::english(),
        }
    }
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
    fn test_new_gen_spec_defaults() {
        let s = spec();
        assert_eq!(s.track, TrackId("cet4".into()));
        assert_eq!(s.kinds.len(), 2);
        assert_eq!(s.count, 3);
        assert_eq!(s.difficulty, (1, 3));
        assert!(s.topic.is_none());
        assert!(s.weak_points.is_empty());
        assert!(s.seed_qid.is_none());
        assert_eq!(s.l2, ambition_core::LangTag::english(), "默认目标语 en");
    }

    #[test]
    fn test_as_str_and_is_choice_for_kind_tags() {
        assert_eq!(QuestionKindTag::SingleChoice.as_str(), "single_choice");
        assert_eq!(QuestionKindTag::FillBlank.as_str(), "fill_blank");
        assert!(QuestionKindTag::SingleChoice.is_choice());
        assert!(!QuestionKindTag::FillBlank.is_choice());
        let json = serde_json::to_value(QuestionKindTag::MultiChoice).expect("序列化");
        assert_eq!(json, serde_json::json!("multi_choice"), "snake_case");
    }

    #[test]
    fn test_is_choice_drives_qc_question_set() {
        assert!(QuestionKindTag::SingleChoice.is_choice());
        assert!(QuestionKindTag::MultiChoice.is_choice());
        assert!(!QuestionKindTag::FillBlank.is_choice());
    }
}
