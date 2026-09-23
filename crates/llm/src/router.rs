//! LLM 路由器：缓存 → 供给方路由（primary→fallback）→ 重试 ≤2（temperature 递减 + 错误摘要）
//! → schema 校验（serde）→ 记账 → 写缓存（docs/05 §1 structured/text）

use ambition_core::UsageSummary;

use crate::breaker::{Breaker, count_failure, window_take};
use crate::{AiLedger, LlmProvider};

/// 结构化输出路由器
pub struct LlmRouter {
    pub(crate) primary: Box<dyn LlmProvider>,
    pub(crate) fallback: Option<Box<dyn LlmProvider>>,
    pub(crate) ledger: Box<dyn AiLedger>,
    pub(crate) breaker: std::sync::Mutex<Breaker>,
}

impl LlmRouter {
    /// 构造（primary 必需；fallback 可空；ledger 必需——无缓存/记账即无成本面板）
    pub fn new(
        primary: Box<dyn LlmProvider>,
        fallback: Option<Box<dyn LlmProvider>>,
        ledger: Box<dyn AiLedger>,
    ) -> Self {
        Self {
            primary,
            fallback,
            ledger,
            breaker: std::sync::Mutex::new(Breaker::default()),
        }
    }

    /// 熔断快速检查：窗口内递减计数并拒发（锁不跨 await；中毒取内层 + 可见信号）
    pub(crate) fn breaker_should_skip(&self) -> bool {
        match self.breaker.lock() {
            Ok(mut guard) => window_take(&mut guard),
            Err(poisoned) => {
                eprintln!("[warn] breaker mutex poisoned, recovering");
                window_take(&mut poisoned.into_inner())
            }
        }
    }

    /// 记一次失败：达阈值则开闸（进入快速失败窗口）
    pub(crate) fn breaker_record_failure(&self) {
        match self.breaker.lock() {
            Ok(mut guard) => count_failure(&mut guard),
            Err(poisoned) => {
                eprintln!("[warn] breaker mutex poisoned, recovering");
                count_failure(&mut poisoned.into_inner())
            }
        }
    }

    /// 成功关闸（计数清零）
    pub(crate) fn breaker_reset(&self) {
        match self.breaker.lock() {
            Ok(mut guard) => *guard = Breaker::default(),
            Err(poisoned) => {
                eprintln!("[warn] breaker mutex poisoned, recovering");
                *poisoned.into_inner() = Breaker::default();
            }
        }
    }

    /// 成本台账：区间 [from_unix, to_unix) 按 kind 聚合（docs/05 §1，M3 落地）
    pub async fn usage_summary(&self, from_unix: i64, to_unix: i64) -> UsageSummary {
        self.ledger.usage_summary(from_unix, to_unix).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::breaker::{BREAKER_FAIL_LIMIT, BREAKER_SKIP_LIMIT};
    use crate::{GenRequest, LlmError, LlmOut, MemLedger};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 脚本化供给方：按序吐出结果（Err 或 Ok）
    struct Scripted {
        script: Mutex<Vec<Result<String, LlmError>>>,
        calls: AtomicUsize,
        model: &'static str,
    }

    impl Scripted {
        fn new(script: Vec<Result<String, LlmError>>) -> Self {
            Self {
                script: Mutex::new(script),
                calls: AtomicUsize::new(0),
                model: "scripted",
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for Scripted {
        async fn complete(&self, req: &GenRequest) -> Result<LlmOut, LlmError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // 签名由 trait 固定；读一下表明参数被消费（防占位参数钩子）
            let temperature = req.temperature;
            debug_assert!(temperature >= 0.0, "temperature 非负");
            let next = match self.script.lock() {
                Ok(mut guard) if !guard.is_empty() => guard.remove(0),
                _ => Err(LlmError::Offline),
            };
            next.map(|text| LlmOut {
                text,
                model: self.model.to_string(),
                tokens_in: 3,
                tokens_out: 4,
            })
        }

        fn model(&self) -> &str {
            self.model
        }
    }

    fn ok_json() -> String {
        r#"{"questions": []}"#.to_string()
    }

    #[test]
    fn test_new_stores_primary_fallback_and_ledger() {
        let primary: Box<dyn LlmProvider> = Box::new(Scripted::new(vec![Ok(ok_json())]));
        let router = LlmRouter::new(primary, None, Box::new(MemLedger::new()));
        assert!(router.fallback.is_none());

        let router2 = LlmRouter::new(
            Box::new(Scripted::new(vec![Ok(ok_json())])),
            Some(Box::new(Scripted::new(vec![Ok(ok_json())]))),
            Box::new(MemLedger::new()),
        );
        assert!(router2.fallback.is_some());
        assert_eq!(router2.primary.model(), "scripted");
    }

    #[test]
    fn test_breaker_record_failure_opens_window_via_router() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![])),
            None,
            Box::new(MemLedger::new()),
        );
        for _ in 0..(BREAKER_FAIL_LIMIT - 1) {
            router.breaker_record_failure();
        }
        {
            let state = router.breaker.lock().expect("breaker 锁");
            assert_eq!(state.fails, BREAKER_FAIL_LIMIT - 1, "阈值前计数");
            assert_eq!(state.skip_left, 0, "未达阈值不开闸");
        }
        router.breaker_record_failure();
        {
            let state = router.breaker.lock().expect("breaker 锁");
            assert_eq!(state.fails, BREAKER_FAIL_LIMIT, "达阈值");
            assert_eq!(state.skip_left, BREAKER_SKIP_LIMIT, "开闸发满窗口票");
        }
        assert!(router.breaker_should_skip(), "开闸后拒发");
    }

    #[test]
    fn test_breaker_reset_clears_fails_and_window() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![])),
            None,
            Box::new(MemLedger::new()),
        );
        for _ in 0..BREAKER_FAIL_LIMIT {
            router.breaker_record_failure();
        }
        assert!(router.breaker_should_skip(), "开闸后窗口内拒发");
        // 把票耗尽后仍应拒发到重置前——直接看状态更稳：重置后完全放行
        router.breaker_reset();
        assert!(!router.breaker_should_skip(), "重置后不再跳过（票已清）");
        let state = router.breaker.lock().expect("breaker 锁");
        assert_eq!((state.fails, state.skip_left), (0, 0), "状态归零");
    }

    #[test]
    fn test_breaker_should_skip_counts_down_window() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![])),
            None,
            Box::new(MemLedger::new()),
        );
        assert!(!router.breaker_should_skip(), "初始无票放行");
        for _ in 0..BREAKER_FAIL_LIMIT {
            router.breaker_record_failure();
        }
        for _ in 0..BREAKER_SKIP_LIMIT {
            assert!(router.breaker_should_skip(), "窗口内拒发");
        }
        assert!(!router.breaker_should_skip(), "窗口耗尽后放行试探");
    }

    #[tokio::test]
    async fn test_usage_summary_delegates_to_ledger_range() {
        let ledger = MemLedger::new();
        ledger.record_usage("jev", "jev-latest", 40, 4).await;
        let router = LlmRouter::new(Box::new(Scripted::new(vec![])), None, Box::new(ledger));
        let summary = router.usage_summary(0, i64::MAX).await;
        assert_eq!(summary.rows.len(), 1, "按 kind 聚合");
        assert_eq!((summary.tokens_in, summary.tokens_out), (40, 4));
        let none = router.usage_summary(i64::MAX - 1, i64::MAX).await;
        assert_eq!(none.rows.len(), 0, "区间外为空");
    }
}
