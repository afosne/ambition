//! StoreLedger：`AiLedger` 的 SQLite 实现（docs/05：trait 定义在 llm，SQLite 实现经 engine 装配）
//! 旁路语义：读写失败 tracing::warn + 返回空/零值，不阻断判题与生成（P-8 可见降级）。

use sqlx::sqlite::SqlitePool;

use ambition_core::{UsageRecord, UsageSummary};
use ambition_llm::AiLedger;

/// 成本台账/缓存的库实现（engine 装配给 LlmRouter 与 JevClient）
pub struct StoreLedger {
    pool: SqlitePool,
}

impl StoreLedger {
    /// 构造（共享 engine 连接池；账本是旁路，不持有独占资源）
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AiLedger for StoreLedger {
    async fn cache_get(&self, cache_key: &str) -> Option<String> {
        match ambition_store::cache_get(&self.pool, cache_key).await {
            Ok(hit) => hit,
            Err(err) => {
                tracing::warn!(error = %err, "llm_cache read failed");
                None
            }
        }
    }

    async fn cache_put(&self, cache_key: &str, payload: &str) {
        if let Err(err) = ambition_store::cache_put(&self.pool, cache_key, payload).await {
            tracing::warn!(error = %err, "llm_cache write failed");
        }
    }

    async fn record_usage(&self, record: UsageRecord) {
        if let Err(err) = ambition_store::record_usage(&self.pool, &record).await {
            tracing::warn!(error = %err, "llm_usage record failed");
        }
    }

    async fn usage_summary(&self, from_unix: i64, to_unix: i64) -> UsageSummary {
        match ambition_store::usage_summary(&self.pool, from_unix, to_unix).await {
            Ok(summary) => summary,
            Err(err) => {
                tracing::warn!(error = %err, "llm_usage summary failed");
                UsageSummary::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_llm::AiLedger;

    async fn ledger() -> StoreLedger {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("内存库");
        ambition_store::init_db(&pool).await.expect("建表");
        StoreLedger::new(pool)
    }

    #[tokio::test]
    async fn test_new_store_ledger_wraps_shared_pool() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("内存库");
        ambition_store::init_db(&pool).await.expect("建表");
        let ledger = StoreLedger::new(pool);
        assert_eq!(
            AiLedger::cache_get(&ledger, "anything").await,
            None,
            "共享池空库读缓存 = 未命中"
        );
    }

    #[tokio::test]
    async fn test_cache_get_returns_none_before_put_then_hit() {
        let ledger = ledger().await;
        assert_eq!(
            AiLedger::cache_get(&ledger, "k").await,
            None,
            "未命中为 None"
        );
        AiLedger::cache_put(&ledger, "k", "v").await;
        assert_eq!(
            AiLedger::cache_get(&ledger, "k").await,
            Some("v".to_string())
        );
    }

    #[tokio::test]
    async fn test_cache_put_overrides_same_key() {
        let ledger = ledger().await;
        AiLedger::cache_put(&ledger, "k", "v1").await;
        AiLedger::cache_put(&ledger, "k", "v2").await;
        assert_eq!(
            AiLedger::cache_get(&ledger, "k").await,
            Some("v2".to_string()),
            "同 key 覆盖（走库主键）"
        );
    }

    #[tokio::test]
    async fn test_record_usage_writes_llm_usage_rows() {
        let ledger = ledger().await;
        AiLedger::record_usage(&ledger, UsageRecord::new("question_gen", "m1", 40, 12)).await;
        AiLedger::record_usage(&ledger, UsageRecord::new("jev", "jev-latest", 7, 2)).await;
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM llm_usage")
            .fetch_one(&ledger.pool)
            .await
            .expect("计数");
        assert_eq!(n, 2, "两行落库");
    }

    #[tokio::test]
    async fn test_usage_summary_aggregates_over_wide_range() {
        let ledger = ledger().await;
        AiLedger::record_usage(&ledger, UsageRecord::new("question_gen", "m1", 100, 20)).await;
        AiLedger::record_usage(&ledger, UsageRecord::new("question_gen", "m1", 30, 5)).await;
        AiLedger::record_usage(&ledger, UsageRecord::new("jev", "jev-latest", 50, 10)).await;
        let summary = AiLedger::usage_summary(&ledger, 0, i64::MAX).await;
        assert_eq!(summary.rows.len(), 2, "按 kind 聚合");
        assert_eq!((summary.tokens_in, summary.tokens_out), (180, 35));
        let future = AiLedger::usage_summary(&ledger, i64::MAX - 1, i64::MAX).await;
        assert_eq!(future.rows.len(), 0, "区间外为空");
    }
}
