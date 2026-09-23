//! 标识与内容散列：Qid/CardSource/CardId/LangTag/ContentHash（docs/02 §1）

use super::*;

/// 内容寻址题目 ID：sha256(题目内容) 前 8 字节 hex（16 字符）
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Qid(pub String);

impl Qid {
    /// 由题目内容生成稳定 id（同内容同 id，用于去重与共享）
    pub fn from_content(content: &str) -> Self {
        let digest = Sha256::digest(content.as_bytes());
        let hex: String = digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Self(hex)
    }
}

/// 卡片来源（docs/02 §1；统一入 08 复习队列）
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardSource {
    /// 义项卡（word_id = lemma 内容定名，09-vocabulary）
    WordSense { word_id: String, sense_id: u64 },
    /// 错题/假对卡
    Question { qid: Qid },
    /// 笔记卡（M6+）
    Note { note_id: String },
    /// 短语卡（M5+）
    Phrase { phrase_id: String },
    /// 句式卡（09-vocabulary）
    Pattern { pattern_id: String },
}

/// 复习卡 ID：source 内容寻址（M4 定案：同源同卡 = 幂等入卡，不走 uuid；docs/02 §1）
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CardId(pub String);

impl CardId {
    /// 由卡片来源推导稳定 id（可读前缀 + 源键；同源恒同卡）
    pub fn from_source(source: &CardSource) -> Self {
        let key = match source {
            CardSource::WordSense { word_id, sense_id } => format!("ws:{word_id}:{sense_id}"),
            CardSource::Question { qid } => format!("q:{}", qid.0),
            CardSource::Note { note_id } => format!("note:{note_id}"),
            CardSource::Phrase { phrase_id } => format!("phrase:{phrase_id}"),
            CardSource::Pattern { pattern_id } => format!("pat:{pattern_id}"),
        };
        Self(key)
    }
}

/// BCP-47 语言标签（"en" / "zh-Hans" …，docs/16-config-i18n）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LangTag(pub String);

impl LangTag {
    /// 默认目标语 en（M1 单语默认，多语种随 M9）
    pub fn english() -> Self {
        Self("en".into())
    }
}

impl Default for LangTag {
    /// 默认 en（非空串：默认值直接可用）
    fn default() -> Self {
        Self::english()
    }
}

/// 内容哈希（sha256 hex，qpack 防篡改 / 缓存 key，docs/02-core-store §1）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContentHash(pub String);

/// 规范 JSON 内容哈希：同一内容（含字段序）恒得同一哈希
pub fn canonical_json_hash<T: serde::Serialize>(value: &T) -> ContentHash {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    ContentHash(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_content_is_stable_and_distinct() {
        assert_eq!(Qid::from_content("abc"), Qid::from_content("abc"));
        assert_eq!(Qid::from_content("abc").0.len(), 16);
        assert_ne!(Qid::from_content("abc"), Qid::from_content("abd"));
    }

    #[test]
    fn test_default_lang_tag_is_english() {
        assert_eq!(LangTag::default(), LangTag::english());
        assert_eq!(Provenance::default(), Provenance::Seed);
    }

    #[test]
    fn test_canonical_json_hash_is_stable_and_order_sensitive() {
        let a = canonical_json_hash(&("x", 1u32));
        let b = canonical_json_hash(&("x", 1u32));
        let c = canonical_json_hash(&("x", 2u32));
        assert_eq!(a, b, "同内容同哈希");
        assert_eq!(a.0.len(), 64, "sha256 全长 hex");
        assert_ne!(a, c, "内容变则哈希变");
    }

    #[test]
    fn test_english_lang_tag_is_en() {
        assert_eq!(LangTag::english().0, "en");
        assert_eq!(LangTag::default(), LangTag::english());
    }

    #[test]
    fn test_from_source_is_stable_and_readable() {
        let qid = Qid("abc123".into());
        let source = CardSource::Question { qid: qid.clone() };
        assert_eq!(CardId::from_source(&source), CardId("q:abc123".to_string()));
        assert_eq!(
            CardId::from_source(&source),
            CardId::from_source(&CardSource::Question { qid }),
            "同源同卡（幂等入卡）"
        );
        assert_eq!(
            CardId::from_source(&CardSource::WordSense {
                word_id: "bank".into(),
                sense_id: 2
            }),
            CardId("ws:bank:2".to_string())
        );
        assert_eq!(
            CardId::from_source(&CardSource::Pattern {
                pattern_id: "not_only".into()
            }),
            CardId("pat:not_only".to_string())
        );
        assert_ne!(
            CardId::from_source(&CardSource::Question {
                qid: Qid("abc123".into())
            }),
            CardId::from_source(&CardSource::Pattern {
                pattern_id: "abc123".into()
            }),
            "不同来源不同卡"
        );
    }
}
