//! 日记门面（M6-W1，docs/03 diary_* + docs/12 §1）：写/读/列——一日一篇（date UNIQUE 单写者）
use crate::*;
use ambition_core::{DiaryEntry, EntryId};

impl Engine {
    /// 写日记（同日覆盖；entry_id 首写保留——store 单写者口径）
    pub async fn diary_write(&self, date: &str, body_md: &str) -> Result<DiaryEntry, EngineError> {
        if date_to_unix(date).is_none() {
            return Err(EngineError::NotFound(format!("非法日期：{date}")));
        }
        let entry = DiaryEntry {
            entry_id: EntryId::new_v7(),
            date: date.to_string(),
            body: body_md.to_string(),
            review: None,
        };
        ambition_store::diary_write(&self.pool, &entry).await?;
        // 同日覆盖时数据库保留首写 entry_id；读回后返回真实持久化行，避免 UI 持有悬空 ID。
        Ok(ambition_store::diary_read(&self.pool, date)
            .await?
            .unwrap_or(entry))
    }

    /// 读某日日记（None = 该日未写）
    pub async fn diary_read(&self, date: &str) -> Result<Option<DiaryEntry>, EngineError> {
        Ok(ambition_store::diary_read(&self.pool, date).await?)
    }

    /// 日记列表（新日在前；review 批注随行）
    pub async fn diary_list(&self, limit: u32) -> Result<Vec<DiaryEntry>, EngineError> {
        Ok(ambition_store::list_diary(&self.pool, i64::from(limit)).await?)
    }

    /// 显式请求日记批改。仅本方法将该条正文提交给已配置的 LLM；不会自动批改或发送历史日记。
    pub async fn diary_review(
        &self,
        date: &str,
    ) -> Result<ambition_core::DiaryReview, EngineError> {
        let entry = ambition_store::diary_read(&self.pool, date)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("diary:{date}")))?;
        let llm = self.llm.as_ref().ok_or(EngineError::Offline)?;
        let schema = serde_json::json!({"type":"object","properties":{"corrected_md":{"type":"string"},"upgrades":{"type":"array","items":{"type":"string"}},"encouragement":{"type":"string"}},"required":["corrected_md","upgrades","encouragement"],"additionalProperties":false});
        let req = ambition_llm::GenRequest::new(
            ambition_llm::LlmKind::Explain,
            "You are a supportive English tutor. Correct only clear errors, suggest at most two concise upgrades, and give one encouraging sentence. Preserve the writer's meaning. Return JSON only.",
            format!(
                "Review this diary entry as explicitly requested by its author:\n\n{}",
                entry.body
            ),
        );
        let mut req = req;
        req.schema = Some(schema);
        #[derive(serde::Deserialize)]
        struct ReviewOutput {
            corrected_md: String,
            upgrades: Vec<String>,
            encouragement: String,
        }
        let output: ReviewOutput = llm.structured(req).await.map_err(from_llm)?;
        let review = ambition_core::DiaryReview {
            corrected_md: output.corrected_md,
            upgrades: output.upgrades.into_iter().take(2).collect(),
            encouragement: output
                .encouragement
                .lines()
                .next()
                .unwrap_or_default()
                .to_string(),
            model: "configured_llm".into(),
            reviewed_at: ambition_core::now_unix(),
        };
        if !ambition_store::diary_set_review(&self.pool, date, &review).await? {
            return Err(EngineError::NotFound(format!("diary:{date}")));
        }
        let _ = self
            .settle(ambition_core::DomainEvent::DiaryReviewed {
                entry_id: entry.entry_id.0,
                date: date.to_string(),
            })
            .await;
        Ok(review)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diary_write_read_roundtrip_same_day() {
        let engine = Engine::open_memory().await.expect("开内存库");
        let entry = engine
            .diary_write("2026-09-29", "今天读了两篇。")
            .await
            .expect("写");
        assert_eq!(entry.body, "今天读了两篇。");
        let got = engine
            .diary_read("2026-09-29")
            .await
            .expect("读")
            .expect("存在");
        assert_eq!(got.entry_id, entry.entry_id, "同日读取");
        assert!(got.review.is_none(), "未批注");
    }

    #[tokio::test]
    async fn test_diary_read_misses_unwritten_date() {
        let engine = Engine::open_memory().await.expect("开内存库");
        assert!(
            engine.diary_read("1999-01-01").await.expect("读").is_none(),
            "未写日 None"
        );
    }

    #[tokio::test]
    async fn test_diary_list_orders_newest_first() {
        let engine = Engine::open_memory().await.expect("开内存库");
        engine.diary_write("2026-09-28", "早").await.expect("写");
        engine.diary_write("2026-09-29", "晚").await.expect("写");
        let rows = engine.diary_list(10).await.expect("列");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].date, "2026-09-29", "新日在前");
    }

    #[tokio::test]
    async fn test_diary_review_requires_explicit_llm_configuration() {
        let engine = Engine::open_memory().await.expect("开内存库");
        engine
            .diary_write("2026-09-29", "private diary")
            .await
            .expect("写日记");
        assert!(matches!(
            engine.diary_review("2026-09-29").await,
            Err(EngineError::Offline)
        ));
        assert!(
            engine
                .diary_read("2026-09-29")
                .await
                .expect("本地读取")
                .unwrap()
                .review
                .is_none()
        );
    }
}
