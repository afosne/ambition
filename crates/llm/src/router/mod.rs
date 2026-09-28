//! LLM 路由器：缓存 → 供给方路由（primary→fallback）→ 重试 ≤2（temperature 递减 + 错误摘要）
//! → schema 校验（serde）→ 记账 → 写缓存（docs/05 §1 structured/text）

pub(crate) mod breaker;

use ambition_core::{UsageSummary, lock_recover};

use self::breaker::{Breaker, count_failure, window_take};
use crate::{AiLedger, LlmProvider};

/// 结构化输出路由器
pub struct LlmRouter {
    pub(crate) primary: Box<dyn LlmProvider>,
    pub(crate) fallback: Option<Box<dyn LlmProvider>>,
    pub(crate) ledger: Box<dyn AiLedger>,
    pub(crate) breaker: std::sync::Mutex<Breaker>,
}

impl LlmRouter {
    /// 主供给方模型标识（记账/prompt 侧展示，M5-W7 Translation.model）
    pub fn model(&self) -> &str {
        self.primary.model()
    }

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

    /// 熔断快速检查：窗口内递减计数并拒发（锁不跨 await；中毒取内层 + 可见信号，core::lock_recover 收口）
    pub(crate) fn breaker_should_skip(&self) -> bool {
        window_take(&mut lock_recover(&self.breaker, "LlmRouter.breaker"))
    }

    /// 记一次失败：达阈值则开闸（进入快速失败窗口）
    pub(crate) fn breaker_record_failure(&self) {
        count_failure(&mut lock_recover(&self.breaker, "LlmRouter.breaker"));
    }

    /// 成功关闸（计数清零）
    pub(crate) fn breaker_reset(&self) {
        *lock_recover(&self.breaker, "LlmRouter.breaker") = Breaker::default();
    }

    /// 成本台账：区间 [from_unix, to_unix) 按 kind 聚合（docs/05 §1，M3 落地）
    pub async fn usage_summary(&self, from_unix: i64, to_unix: i64) -> UsageSummary {
        self.ledger.usage_summary(from_unix, to_unix).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_model_reports_primary_provider_identifier() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![])),
            None,
            Box::new(MemLedger::new()),
        );
        assert!(!router.model().is_empty(), "主供给方模型标识可读");
    }

    use super::*;
    use crate::common::*;
    use crate::router::breaker::{BREAKER_FAIL_LIMIT, BREAKER_SKIP_LIMIT};
    use crate::{MemLedger, UsageRecord};

    /// 脚本化供给方：按序吐出结果（Err 或 Ok）
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
        ledger
            .record_usage(UsageRecord::new("jev", "jev-latest", 40, 4))
            .await;
        let router = LlmRouter::new(Box::new(Scripted::new(vec![])), None, Box::new(ledger));
        let summary = router.usage_summary(0, i64::MAX).await;
        assert_eq!(summary.rows.len(), 1, "按 kind 聚合");
        assert_eq!((summary.tokens_in, summary.tokens_out), (40, 4));
        let none = router.usage_summary(i64::MAX - 1, i64::MAX).await;
        assert_eq!(none.rows.len(), 0, "区间外为空");
    }
}
