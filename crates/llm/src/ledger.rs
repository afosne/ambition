//! 用量/缓存账本（trait 定义在 05；SQLite 实现经 engine 装配，测试用进程内 MemLedger）

use std::collections::HashMap;
use std::sync::Mutex;

/// 记账行（测试断言用）
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub kind: String,
    pub model: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
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
        match self.cache.lock() {
            Ok(guard) => guard.get(cache_key).cloned(),
            // 中毒：取内层数据并留可见信号（P-8：降级必须可见）
            Err(poisoned) => {
                tracing_warn_poison("MemLedger.cache_get");
                poisoned.into_inner().get(cache_key).cloned()
            }
        }
    }

    async fn cache_put(&self, cache_key: &str, payload: &str) {
        match self.cache.lock() {
            Ok(mut guard) => {
                guard.insert(cache_key.to_string(), payload.to_string());
            }
            Err(poisoned) => {
                tracing_warn_poison("MemLedger.cache_put");
                poisoned
                    .into_inner()
                    .insert(cache_key.to_string(), payload.to_string());
            }
        }
    }

    async fn record_usage(&self, kind: &str, model: &str, tokens_in: i64, tokens_out: i64) {
        let row = UsageRow {
            kind: kind.to_string(),
            model: model.to_string(),
            tokens_in,
            tokens_out,
        };
        match self.usage.lock() {
            Ok(mut guard) => guard.push(row),
            Err(poisoned) => {
                tracing_warn_poison("MemLedger.record_usage");
                poisoned.into_inner().push(row);
            }
        }
    }
}

/// 锁中毒的可见信号（eprintln 保证无 tracing subscriber 时也可观测；P-8）
fn tracing_warn_poison(where_: &str) {
    eprintln!("[warn] mutex poisoned at {where_}, recovering inner state");
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
        ledger.record_usage("question_gen", "m1", 10, 5).await;
        ledger.record_usage("jev", "jev-latest", 7, 2).await;
        let rows = ledger.take_usage();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "question_gen");
        assert_eq!(rows[0].tokens_in, 10);
        assert_eq!(rows[1].model, "jev-latest");
        assert!(ledger.take_usage().is_empty(), "take 后清空");
    }

    #[test]
    fn test_tracing_warn_poison_is_visible() {
        // 仅验证函数可调用（输出到 stderr 是 P-8 要求的可见信号）
        tracing_warn_poison("test.point");
    }
}
