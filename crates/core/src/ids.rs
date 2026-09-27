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

/// 母语默认标签（docs/16 §1：l1=zh-Hans；TODO(16-config)：config 落地后由调用方注入，占位不再取常量）
pub const DEFAULT_L1: &str = "zh-Hans";

impl LangTag {
    /// 默认目标语 en（M1 单语默认，多语种随 M9）
    pub fn english() -> Self {
        Self("en".into())
    }

    /// 语言名，用 `written_in` 语言书写（prompt 占位 `{{l1}}/{{l2}}` 用，docs/05 §1）。
    /// 收录 en/zh 两种书写语言 × en/zh/ja；未收录组合回退语言标签本身（如 "fr"）。
    pub fn name_in(&self, written_in: &str) -> String {
        let target = if matches!(self.0.as_str(), "zh" | "zh-Hans" | "zh-Hant") {
            "zh"
        } else {
            self.0.as_str()
        };
        let name = match (written_in, target) {
            ("en", "en") => "English",
            ("en", "zh") => "Chinese",
            ("en", "ja") => "Japanese",
            ("zh", "en") => "英语",
            ("zh", "zh") => "中文",
            ("zh", "ja") => "日语",
            _ => return self.0.clone(),
        };
        name.to_string()
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
    fn test_name_in_renders_requested_language_with_tag_fallback() {
        let en = LangTag::english();
        assert_eq!(en.name_in("en"), "English");
        assert_eq!(en.name_in("zh"), "英语");
        let zh = LangTag(DEFAULT_L1.into());
        assert_eq!(zh.name_in("en"), "Chinese", "zh-Hans 归 zh 谱系");
        assert_eq!(zh.name_in("zh"), "中文");
        assert_eq!(LangTag("ja".into()).name_in("en"), "Japanese");
        assert_eq!(LangTag("fr".into()).name_in("en"), "fr", "未收录回退标签");
        assert_eq!(DEFAULT_L1, "zh-Hans", "l1 默认随 docs/16 §1");
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
