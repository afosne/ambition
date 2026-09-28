//! 翻译工作台门面（M5-W7，docs/03 S3 + docs/11 §2）：LLM 出译/对齐 + 我来翻评分 + 历史/收藏 + 句转练习
//! 译文落库走 TranslateRepo（tx_id 内容哈希与 llm_cache key 同源——同文同向不重复调 LLM，docs/11 §4）
use ambition_core::{QuestionKind, Translation};

use crate::*;

impl Engine {
    /// LLM 出译（structured target；缓存由 llm run_loop 按 effective_cache_key 自动命中；落 translations）
    pub async fn translate(
        &self,
        text: &str,
        opts: &ambition_translate::TranslateOpts,
    ) -> Result<Translation, EngineError> {
        let Some(llm) = &self.llm else {
            return Err(EngineError::Offline);
        };
        let (instruction, prompt) = ambition_translate::render_translate_prompt(opts, text);
        let mut req =
            ambition_llm::GenRequest::new(ambition_llm::LlmKind::Translate, instruction, prompt);
        req.schema = Some(ambition_translate::translate_schema());
        let wire: ambition_translate::TranslateWire =
            llm.structured(req).await.map_err(from_llm)?;
        let hits = opts.glossary_hits(text, &wire.target);
        // ContentHash 已是十六进制串——tx_id 与 llm_cache key 同源（同文同向不重复调 LLM）
        let tx_id = format!(
            "tx-{}",
            ambition_core::canonical_json_hash(&serde_json::json!({
                "text": text,
                "dir": format!("{}>{}", opts.l1.0, opts.l2.0),
                "glossary": opts.glossary,
            }))
            .0
        );
        let tx = Translation {
            tx_id,
            source: text.to_string(),
            target: wire.target,
            dir: ambition_core::TranslationDirection {
                from: opts.l1.clone(),
                to: opts.l2.clone(),
            },
            model: llm.model().to_string(),
            glossary_hits: hits,
        };
        ambition_store::insert_translation(&self.pool, &tx).await?;
        Ok(tx)
    }

    /// 句/短语对齐（LLM 结构化 segments——对照学习面，只切段不改写，docs/11 §2）
    pub async fn translate_align(
        &self,
        source: &str,
        target: &str,
        opts: &ambition_translate::TranslateOpts,
    ) -> Result<ambition_translate::Alignment, EngineError> {
        let Some(llm) = &self.llm else {
            return Err(EngineError::Offline);
        };
        let (instruction, prompt) = ambition_translate::render_align_prompt(opts, source, target);
        let mut req =
            ambition_llm::GenRequest::new(ambition_llm::LlmKind::Translate, instruction, prompt);
        req.schema = Some(ambition_translate::align_schema());
        let alignment: ambition_translate::Alignment =
            llm.structured(req).await.map_err(from_llm)?;
        Ok(alignment)
    }

    /// 我来翻（参考译文命中缓存复用 + 差异 + 评分）
    /// l2=en → 07 translation composite（唯一评分路径）；其它语种 grade=None 降级（多语 LLM 粗判随多语波次——docs/11 §2）
    /// 评分发布 AttemptGraded（复用事件、无新事件——docs/11 §3，gamify 正常 Xp/连击）
    pub async fn translate_my_turn(
        &self,
        source: &str,
        mine: &str,
        opts: &ambition_translate::TranslateOpts,
    ) -> Result<ambition_translate::CompareResult, EngineError> {
        let reference = self.translate(source, opts).await?;
        let diff = ambition_translate::diff(&reference.target, mine);
        let grade = if opts.l2.0 == "en" {
            let question = Question::new(
                format!("Translate the following sentence into {}.", opts.l2.0),
                QuestionKind::Translation(ambition_core::TranslationBody {
                    direction: ambition_core::TranslationDirection {
                        from: opts.l1.clone(),
                        to: opts.l2.clone(),
                    },
                    source_text: source.to_string(),
                    reference: reference.target.clone(),
                }),
                3,
                vec!["translation".to_string(), "from:tx".to_string()],
                ambition_core::Provenance::Generated,
            );
            let grade = self
                .grader
                .grade(&question, &Answer::Text(mine.to_string()))
                .await;
            ambition_core::lock_recover(&self.events, "Engine.events").push(
                DomainEvent::AttemptGraded {
                    attempt_id: format!("tx-{}", reference.tx_id),
                    qid: question.qid,
                    verdict: grade.verdict,
                    correct: grade.verdict == Verdict::Correct,
                },
            );
            Some(grade)
        } else {
            None
        };
        Ok(ambition_translate::CompareResult {
            reference,
            diff,
            grade,
        })
    }

    /// 历史/检索（q 模糊命中源/目标；None = 近 limit 全量倒序）
    pub async fn translate_history(
        &self,
        q: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Translation>, EngineError> {
        Ok(ambition_store::search_translations(&self.pool, q, i64::from(limit)).await?)
    }

    /// 收藏开关（on upsert / off delete，幂等）
    pub async fn translate_favorite(&self, tx_id: &str, on: bool) -> Result<(), EngineError> {
        ambition_store::favorite_tx(&self.pool, tx_id, on).await?;
        Ok(())
    }

    /// 句转练习（工作台句 → Translation 题；tag from:tx 随卷可溯源，docs/11 §2）
    pub async fn translate_to_practice(&self, tx_id: &str) -> Result<Question, EngineError> {
        let tx = ambition_store::get_translation(&self.pool, tx_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("tx:{tx_id}")))?;
        Ok(Question::new(
            format!("Translate the following sentence into {}.", tx.dir.to.0),
            QuestionKind::Translation(ambition_core::TranslationBody {
                direction: tx.dir.clone(),
                source_text: tx.source.clone(),
                reference: tx.target.clone(),
            }),
            3,
            vec!["translation".to_string(), "from:tx".to_string()],
            ambition_core::Provenance::Generated,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{LangTag, TranslationDirection};

    fn en_opts() -> ambition_translate::TranslateOpts {
        ambition_translate::TranslateOpts::new(LangTag("zh-Hans".into()), LangTag::english())
    }

    fn tx(tx_id: &str) -> Translation {
        Translation {
            tx_id: tx_id.to_string(),
            source: "我喜欢春天。".to_string(),
            target: "I like spring.".to_string(),
            dir: TranslationDirection {
                from: LangTag("zh-Hans".into()),
                to: LangTag::english(),
            },
            model: "m1".to_string(),
            glossary_hits: 0,
        }
    }

    #[tokio::test]
    async fn test_translate_offline_without_llm() {
        let engine = Engine::open_memory().await.expect("内存库");
        let err = engine
            .translate("我喜欢春天。", &en_opts())
            .await
            .expect_err("无 LLM 必须报错");
        assert!(
            matches!(err, EngineError::Offline),
            "离线配置提示口径（P-8）"
        );
    }

    #[tokio::test]
    async fn test_translate_align_offline_without_llm() {
        let engine = Engine::open_memory().await.expect("内存库");
        let err = engine
            .translate_align("我喜欢春天。", "I like spring.", &en_opts())
            .await
            .expect_err("无 LLM 必须报错");
        assert!(matches!(err, EngineError::Offline), "离线配置提示口径");
    }

    #[tokio::test]
    async fn test_translate_my_turn_offline_propagates_before_grading() {
        let engine = Engine::open_memory().await.expect("内存库");
        let err = engine
            .translate_my_turn("我喜欢春天。", "I enjoy spring.", &en_opts())
            .await
            .expect_err("先出参考译文——离线即报");
        assert!(matches!(err, EngineError::Offline), "不伪造参考译文与评分");
    }

    #[tokio::test]
    async fn test_translate_history_roundtrips_from_store() {
        let engine = Engine::open_memory().await.expect("内存库");
        ambition_store::insert_translation(&engine.pool, &tx("tx-1"))
            .await
            .expect("落库");
        ambition_store::insert_translation(&engine.pool, &tx("tx-2"))
            .await
            .expect("落库2");
        let hit = engine
            .translate_history(Some("春天"), 10)
            .await
            .expect("历史");
        assert_eq!(hit.len(), 2, "q 命中两行");
        let all = engine.translate_history(None, 1).await.expect("全量");
        assert_eq!(all.len(), 1, "limit 生效");
    }

    #[tokio::test]
    async fn test_translate_favorite_roundtrips() {
        let engine = Engine::open_memory().await.expect("内存库");
        ambition_store::insert_translation(&engine.pool, &tx("tx-1"))
            .await
            .expect("落库");
        engine.translate_favorite("tx-1", true).await.expect("收藏");
        let favs = ambition_store::favorite_translations(&engine.pool)
            .await
            .expect("收藏列表");
        assert_eq!(favs.len(), 1, "收藏可回读");
        engine
            .translate_favorite("tx-1", false)
            .await
            .expect("取消");
        let favs = ambition_store::favorite_translations(&engine.pool)
            .await
            .expect("收藏列表");
        assert!(favs.is_empty(), "取消后清空");
    }

    #[tokio::test]
    async fn test_translate_to_practice_builds_tagged_question() {
        let engine = Engine::open_memory().await.expect("内存库");
        ambition_store::insert_translation(&engine.pool, &tx("tx-7"))
            .await
            .expect("落库");
        let q = engine.translate_to_practice("tx-7").await.expect("转题");
        let QuestionKind::Translation(body) = &q.kind else {
            panic!("必须是翻译题");
        };
        assert_eq!(body.source_text, "我喜欢春天。", "源文回填");
        assert_eq!(body.reference, "I like spring.", "参考译文回填");
        assert!(
            q.tags.contains(&"from:tx".to_string()),
            "tag from:tx 可溯源"
        );
        assert!(q.tags.contains(&"translation".to_string()), "翻译题标签");
        let missing = engine
            .translate_to_practice("nope")
            .await
            .expect_err("未知 tx 必须报错");
        assert!(matches!(missing, EngineError::NotFound(_)), "NotFound 口径");
    }
}
