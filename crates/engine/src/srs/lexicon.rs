//! 内置词库/句式装载与入卡：Sense/Pattern/word_detail/enroll_builtin_cards
//! （docs/09 §1/§3 M4 最小可用；data/ 内容经 include_str 编译期内嵌——同 seed 题库红线 #17/#19）

use super::*;
use ambition_core::CardSource;

/// 内置义项（一词多义按义项拆行，docs/09 §1 Sense）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sense {
    pub word_id: String,
    pub sense_id: u64,
    pub pos: String,
    pub def_zh: String,
    pub def_en: String,
    #[serde(default)]
    pub examples: Vec<String>,
}

/// 义项文件（根带 schema_version=1，docs/09 §1）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SenseFile {
    schema_version: u32,
    senses: Vec<Sense>,
}

/// 例对（plain ↔ patterned，docs/09 §3）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExamplePair {
    pub plain: String,
    pub patterned: String,
}

/// 固定句式（M4 内置，docs/09 §3 Pattern）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pattern {
    pub pattern_id: String,
    pub form: String,
    pub kind: String,
    #[serde(default)]
    pub level_tags: Vec<String>,
    pub note_zh: String,
    #[serde(default)]
    pub examples: Vec<ExamplePair>,
    #[serde(default)]
    pub traps: Vec<String>,
}

/// 句式文件（根带 schema_version=1）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PatternFile {
    schema_version: u32,
    patterns: Vec<Pattern>,
}

/// 义项文件 schema 版本（docs/09 §1；变更需同步 data 文件根）
pub const SENSES_SCHEMA_VERSION: u32 = 1;
/// 句式文件 schema 版本
pub const PATTERNS_SCHEMA_VERSION: u32 = 1;

/// 内嵌义项（编译期固定；坏内容在 loader 显式 Err，assemble 降级 warn）
fn senses_raw() -> Result<Vec<Sense>, EngineError> {
    parse_senses(include_str!("../../../../data/lexicon/en/senses.json"))
}

/// 内嵌句式
fn patterns_raw() -> Result<Vec<Pattern>, EngineError> {
    parse_patterns(include_str!("../../../../data/patterns/en/patterns.json"))
}

/// 句式卡背面（pattern_id → note_zh + form；未知 id → None，复习条目退源键）
pub(crate) fn pattern_note(pattern_id: &str) -> Option<String> {
    patterns_raw()
        .ok()?
        .into_iter()
        .find(|p| p.pattern_id == pattern_id)
        .map(|p| format!("{}（{}）", p.note_zh, p.form))
}

/// 解析义项 JSON（schema_version 校验 + 结构校验；坏文件显式 Err 不静默）
fn parse_senses(text: &str) -> Result<Vec<Sense>, EngineError> {
    let file: SenseFile = serde_json::from_str(text)
        .map_err(|err| EngineError::Vocab(format!("senses.json 解析失败: {err}")))?;
    if file.schema_version != SENSES_SCHEMA_VERSION {
        return Err(EngineError::Vocab(format!(
            "senses.json schema_version {} ≠ {SENSES_SCHEMA_VERSION}",
            file.schema_version
        )));
    }
    if file.senses.is_empty() {
        return Err(EngineError::Vocab("senses.json 义项为空".into()));
    }
    Ok(file.senses)
}

/// 解析句式 JSON（同上口径）
fn parse_patterns(text: &str) -> Result<Vec<Pattern>, EngineError> {
    let file: PatternFile = serde_json::from_str(text)
        .map_err(|err| EngineError::Vocab(format!("patterns.json 解析失败: {err}")))?;
    if file.schema_version != PATTERNS_SCHEMA_VERSION {
        return Err(EngineError::Vocab(format!(
            "patterns.json schema_version {} ≠ {PATTERNS_SCHEMA_VERSION}",
            file.schema_version
        )));
    }
    if file.patterns.is_empty() {
        return Err(EngineError::Vocab("patterns.json 句式为空".into()));
    }
    Ok(file.patterns)
}

impl Engine {
    /// 义项查询（word_detail 门面，docs/03 §2 / docs/09 §1；未知词 → 空表）
    pub async fn word_detail(&self, lemma: &str) -> Result<Vec<Sense>, EngineError> {
        let senses = senses_raw()?;
        Ok(senses
            .into_iter()
            .filter(|sense| sense.word_id == lemma)
            .collect())
    }

    /// 内置义项/句式幂等入卡（assemble 调用；**已有卡不覆盖**——保留 FSRS 调度，docs/03 §1 M4 注记）
    pub(crate) async fn enroll_builtin_cards(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // 义项卡（坏文件仅 warn 不阻断 open，docs/03 §1）
        match senses_raw() {
            Ok(senses) => {
                for sense in &senses {
                    let source = CardSource::WordSense {
                        word_id: sense.word_id.clone(),
                        sense_id: sense.sense_id,
                    };
                    if let Err(err) = self.enroll_card(source, now).await {
                        tracing::warn!(error = %err, "sense card enroll failed");
                    }
                }
            }
            Err(err) => tracing::warn!(error = %err, "senses.json 装载失败（跳过义项入卡）"),
        }
        // 句式卡
        match patterns_raw() {
            Ok(patterns) => {
                for pattern in &patterns {
                    let source = CardSource::Pattern {
                        pattern_id: pattern.pattern_id.clone(),
                    };
                    if let Err(err) = self.enroll_card(source, now).await {
                        tracing::warn!(error = %err, "pattern card enroll failed");
                    }
                }
            }
            Err(err) => tracing::warn!(error = %err, "patterns.json 装载失败（跳过句式入卡）"),
        }
    }

    /// 单卡幂等入卡（已存在含调度进度 → 跳过；不存在 → 新卡 due_at=now 立即进队列）
    async fn enroll_card(&self, source: CardSource, now: i64) -> Result<(), sqlx::Error> {
        let card_id = ambition_core::CardId::from_source(&source);
        match ambition_store::get_card(&self.pool, &card_id.0).await {
            Ok(Some(_)) => Ok(()),
            Ok(None) => {
                let card = ambition_srs::Card::new(source, now);
                ambition_store::upsert_card(&self.pool, &card).await
            }
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_senses_raw_loads_builtin_with_content() {
        let senses = senses_raw().expect("内嵌义项可读");
        assert!(senses.len() >= 20, "内置 ≥20 义项: {}", senses.len());
        assert!(senses.iter().any(|s| s.word_id == "bank"), "含 bank");
    }

    #[test]
    fn test_patterns_raw_loads_builtin_with_content() {
        let patterns = patterns_raw().expect("内嵌句式可读");
        assert!(patterns.len() >= 6, "内置 ≥6 句式: {}", patterns.len());
        assert!(
            patterns.iter().any(|p| p.pattern_id == "not_only_but_also"),
            "含 not only 句式"
        );
    }

    #[tokio::test]
    async fn test_enroll_card_skips_existing_without_overwriting() {
        let engine = Engine::open_memory().await.expect("打开");
        let now = 2_000_000_000;
        let source = CardSource::WordSense {
            word_id: "zzz-unique".into(),
            sense_id: 1,
        };
        // 首次入卡
        engine.enroll_card(source.clone(), now).await.expect("入卡");
        let card_id = ambition_core::CardId::from_source(&source);
        let first = ambition_store::get_card(&engine.pool, &card_id.0)
            .await
            .expect("读卡")
            .expect("存在");
        assert_eq!(first.due_at, now);
        // 再次入卡跳过（不同 now 不覆盖）
        engine
            .enroll_card(source.clone(), now + 999)
            .await
            .expect("幂等");
        let second = ambition_store::get_card(&engine.pool, &card_id.0)
            .await
            .expect("读卡")
            .expect("存在");
        assert_eq!(second.due_at, now, "已存在不覆盖调度");
    }

    #[test]
    fn test_parse_senses_validates_schema_and_content() {
        let senses = parse_senses(include_str!("../../../../data/lexicon/en/senses.json"))
            .expect("内置义项可解析");
        assert!(senses.len() >= 20, "内置义项 ≥20（实际 {}）", senses.len());
        // 验证点：bank/light/run 每词 ≥2 义项、def 互异（docs/09 §5 抽检）
        for word in ["bank", "light", "run"] {
            let defs: Vec<&str> = senses
                .iter()
                .filter(|s| s.word_id == word)
                .map(|s| s.def_zh.as_str())
                .collect();
            assert!(defs.len() >= 2, "{word} 义项 ≥2: {defs:?}");
            let unique: std::collections::HashSet<&str> = defs.iter().copied().collect();
            assert_eq!(unique.len(), defs.len(), "{word} def 互异: {defs:?}");
        }
        // 坏 schema 版本显式 Err
        let bad = r#"{"schema_version": 99, "senses": []}"#;
        assert!(
            matches!(parse_senses(bad), Err(EngineError::Vocab(_))),
            "版本不符显式 Err"
        );
        let broken = r#"{"schema_version": 1, "#;
        assert!(parse_senses(broken).is_err(), "坏 JSON 显式 Err");
        let empty = r#"{"schema_version": 1, "senses": []}"#;
        assert!(parse_senses(empty).is_err(), "空义项 Err");
    }

    #[test]
    fn test_parse_patterns_validates_schema_and_content() {
        let patterns = parse_patterns(include_str!("../../../../data/patterns/en/patterns.json"))
            .expect("内置句式可解析");
        assert!(
            patterns.len() >= 6,
            "内置句式 ≥6（实际 {}）",
            patterns.len()
        );
        for p in &patterns {
            assert!(
                !p.form.contains(' ') || !p.note_zh.is_empty(),
                "{}",
                p.pattern_id
            );
            assert!(!p.examples.is_empty(), "{} 需要例对", p.pattern_id);
        }
        let bad = r#"{"schema_version": 2, "patterns": []}"#;
        assert!(parse_patterns(bad).is_err(), "版本不符显式 Err");
        let empty = r#"{"schema_version": 1, "patterns": []}"#;
        assert!(parse_patterns(empty).is_err(), "空句式 Err");
    }

    #[test]
    fn test_pattern_note_returns_note_with_form() {
        let note = pattern_note("not_only_but_also").expect("内置句式存在");
        assert!(note.contains("不但"), "中文讲解: {note}");
        assert!(note.contains("not only"), "句式形式: {note}");
        assert!(pattern_note("no-such-pattern").is_none(), "未知句式 None");
    }

    #[tokio::test]
    async fn test_word_detail_filters_by_lemma() {
        let engine = Engine::open_memory().await.expect("打开");
        let bank = engine.word_detail("bank").await.expect("查询");
        assert!(bank.len() >= 2, "bank 多义: {} 义", bank.len());
        assert_eq!(bank[0].word_id, "bank");
        let none = engine.word_detail("zzz-not-a-word").await.expect("查询");
        assert!(none.is_empty(), "未知词空表");
    }

    #[tokio::test]
    async fn test_enroll_builtin_cards_is_idempotent_and_keeps_schedule() {
        let engine = Engine::open_memory().await.expect("打开");
        engine.enroll_builtin_cards().await;
        let cards = ambition_store::list_cards(&engine.pool)
            .await
            .expect("全量");
        let senses_count = cards
            .iter()
            .filter(|c| matches!(c.source, CardSource::WordSense { .. }))
            .count();
        let patterns_count = cards
            .iter()
            .filter(|c| matches!(c.source, CardSource::Pattern { .. }))
            .count();
        assert!(senses_count >= 20, "义项卡 ≥20: {senses_count}");
        assert!(patterns_count >= 6, "句式卡 ≥6: {patterns_count}");
        // 幂等：重复装配不翻倍
        engine.enroll_builtin_cards().await;
        let again = ambition_store::list_cards(&engine.pool)
            .await
            .expect("全量");
        assert_eq!(again.len(), cards.len(), "幂等入卡不重复");
        // 已有卡调度不被覆盖：模拟复习后重装
        let card_id = ambition_core::CardId::from_source(&CardSource::WordSense {
            word_id: "bank".into(),
            sense_id: 1,
        });
        let mut card = ambition_store::get_card(&engine.pool, &card_id.0)
            .await
            .expect("读卡")
            .expect("存在");
        ambition_srs::schedule(&mut card, ambition_srs::Rating::Good, 2_000_000_000).expect("调度");
        ambition_store::upsert_card(&engine.pool, &card)
            .await
            .expect("写");
        engine.enroll_builtin_cards().await;
        let after = ambition_store::get_card(&engine.pool, &card_id.0)
            .await
            .expect("读卡")
            .expect("存在");
        assert_eq!(after.due_at, card.due_at, "重装不覆盖调度进度");
        assert!(after.fsrs.is_some(), "FSRS 状态保留");
    }

    #[tokio::test]
    async fn test_assemble_survives_when_builtin_cards_enrolled() {
        // assemble 路径包含入卡（坏文件仅 warn 不阻断——open 成功即证）
        let engine = Engine::open_memory().await.expect("打开");
        assert!(
            ambition_store::list_cards(&engine.pool)
                .await
                .expect("读卡")
                .len()
                >= 26,
            "open 时内置卡已入队（义项 + 句式）"
        );
        drop(engine);
    }
}
