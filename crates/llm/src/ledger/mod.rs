//! 用量/缓存账本（trait 定义在 05；SQLite 实现经 engine 装配，测试用进程内 MemLedger）

use ambition_core::{UsageRecord, UsageSummary, UsageTotal, lock_recover, now_unix};
use std::collections::HashMap;
use std::sync::Mutex;

/// 记账行（测试断言用）
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub kind: String,
    pub model: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
    /// 记账时刻（unix 秒，区间汇总的过滤维度，M3）
    pub at_unix: i64,
}

/// 行集按 kind 聚合（[from, to) 过滤；区间外行丢弃）
fn summarize_rows(rows: &[UsageRow], from_unix: i64, to_unix: i64) -> UsageSummary {
    let mut grouped: std::collections::BTreeMap<String, (i64, i64)> =
        std::collections::BTreeMap::new();
    for row in rows {
        if row.at_unix < from_unix || row.at_unix >= to_unix {
            continue;
        }
        let entry = grouped.entry(row.kind.clone()).or_insert((0, 0));
        entry.0 += row.tokens_in;
        entry.1 += row.tokens_out;
    }
    UsageSummary::from_rows(
        grouped
            .into_iter()
            .map(|(kind, (tokens_in, tokens_out))| UsageTotal {
                kind,
                tokens_in,
                tokens_out,
            })
            .collect(),
    )
}

/// 进程内账本（单测 / 无库形态；生产 = engine::StoreLedger 实现同一 trait）
#[derive(Default)]
pub struct MemLedger {
    cache: Mutex<HashMap<String, String>>,
    usage: Mutex<Vec<UsageRow>>,
}

impl MemLedger {
    /// 构造空账本
    pub fn new() -> Self {
        Self::default()
    }

    /// 取走全部记账行（测试断言）
    pub fn take_usage(&self) -> Vec<UsageRow> {
        match self.usage.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

#[async_trait::async_trait]
impl crate::AiLedger for MemLedger {
    async fn cache_get(&self, cache_key: &str) -> Option<String> {
        // 锁中毒取内层 + 可见信号（P-8；core::lock_recover 收口，不静默）
        lock_recover(&self.cache, "MemLedger.cache_get")
            .get(cache_key)
            .cloned()
    }

    async fn cache_put(&self, cache_key: &str, payload: &str) {
        lock_recover(&self.cache, "MemLedger.cache_put")
            .insert(cache_key.to_string(), payload.to_string());
    }

    async fn record_usage(&self, record: UsageRecord) {
        let row = UsageRow {
            kind: record.kind,
            model: record.model,
            tokens_in: record.tokens_in,
            tokens_out: record.tokens_out,
            at_unix: now_unix(),
        };
        lock_recover(&self.usage, "MemLedger.record_usage").push(row);
    }

    async fn usage_summary(&self, from_unix: i64, to_unix: i64) -> UsageSummary {
        let rows = lock_recover(&self.usage, "MemLedger.usage_summary").clone();
        summarize_rows(&rows, from_unix, to_unix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AiLedger;

    #[test]
    fn test_new_starts_empty() {
        let ledger = MemLedger::new();
        assert!(ledger.take_usage().is_empty());
    }

    #[tokio::test]
    async fn test_cache_put_writes_and_overrides_key() {
        let ledger = MemLedger::new();
        ledger.cache_put("alpha", "v1").await;
        assert_eq!(ledger.cache_get("alpha").await, Some("v1".to_string()));
        ledger.cache_put("alpha", "v2").await;
        assert_eq!(
            ledger.cache_get("alpha").await,
            Some("v2".to_string()),
            "同 key 覆盖写"
        );
        assert_eq!(ledger.cache_get("beta").await, None, "其它 key 未命中");
    }

    #[test]
    fn test_take_usage_returns_rows_and_clears() {
        let ledger = MemLedger::new();
        ledger.take_usage(); // 空账本取走不报错
        let rows = ledger.take_usage();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_cache_get_miss_then_hit_after_put() {
        let ledger = MemLedger::new();
        assert_eq!(ledger.cache_get("k").await, None, "未命中为 None");
        ledger.cache_put("k", "v1").await;
        assert_eq!(ledger.cache_get("k").await, Some("v1".to_string()));
        ledger.cache_put("k", "v2").await;
        assert_eq!(
            ledger.cache_get("k").await,
            Some("v2".to_string()),
            "同 key 覆盖"
        );
    }

    #[tokio::test]
    async fn test_record_usage_appends_rows_and_take_empties() {
        let ledger = MemLedger::new();
        ledger
            .record_usage(UsageRecord::new("question_gen", "m1", 10, 5))
            .await;
        ledger
            .record_usage(UsageRecord::new("jev", "jev-latest", 7, 2))
            .await;
        let rows = ledger.take_usage();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "question_gen");
        assert_eq!(rows[0].tokens_in, 10);
        assert_eq!(rows[1].model, "jev-latest");
        assert!(ledger.take_usage().is_empty(), "take 后清空");
    }

    #[test]
    fn test_summarize_rows_filters_range_and_groups_kind() {
        let rows = vec![
            UsageRow {
                kind: "llm".into(),
                model: "m".into(),
                tokens_in: 100,
                tokens_out: 20,
                at_unix: 1_000,
            },
            UsageRow {
                kind: "llm".into(),
                model: "m".into(),
                tokens_in: 50,
                tokens_out: 5,
                at_unix: 2_000,
            },
            UsageRow {
                kind: "jev".into(),
                model: "j".into(),
                tokens_in: 30,
                tokens_out: 3,
                at_unix: 1_500,
            },
            // 区间外（to 开区间）
            UsageRow {
                kind: "llm".into(),
                model: "m".into(),
                tokens_in: 999,
                tokens_out: 999,
                at_unix: 3_000,
            },
        ];
        let all = summarize_rows(&rows, 0, i64::MAX);
        assert_eq!(all.tokens_in, 100 + 50 + 30 + 999);
        assert_eq!(all.rows.len(), 2, "按 kind 聚合");
        let win = summarize_rows(&rows, 1_000, 2_000);
        assert_eq!(win.tokens_in, 130, "[from,to) 开区间过滤");
        assert_eq!(win.rows.len(), 2);
        let future = summarize_rows(&rows, 5_000, 6_000);
        assert_eq!(future.rows.len(), 0, "空区间零值");
    }

    #[tokio::test]
    async fn test_usage_summary_mem_ledger_aggregates_recorded_rows() {
        let ledger = MemLedger::new();
        ledger
            .record_usage(UsageRecord::new("question_gen", "m1", 10, 5))
            .await;
        ledger
            .record_usage(UsageRecord::new("jev", "jev-latest", 7, 2))
            .await;
        ledger
            .record_usage(UsageRecord::new("question_gen", "m1", 30, 5))
            .await;
        let summary = ledger.usage_summary(0, i64::MAX).await;
        assert_eq!(summary.rows.len(), 2, "两 kind");
        assert_eq!(summary.tokens_in, 47);
        assert_eq!(summary.tokens_out, 12);
        let llm = summary
            .rows
            .iter()
            .find(|r| r.kind == "question_gen")
            .expect("question_gen 行");
        assert_eq!((llm.tokens_in, llm.tokens_out), (40, 10));
        let empty = ledger.usage_summary(i64::MAX - 1, i64::MAX).await;
        assert_eq!(empty.rows.len(), 0, "未来区间为空（at_unix < from）");
    }
}
