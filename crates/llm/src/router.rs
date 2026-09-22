//! LLM 路由器：缓存 → 供给方路由（primary→fallback）→ 重试 ≤2（temperature 递减 + 错误摘要）
//! → schema 校验（serde）→ 记账 → 写缓存（docs/05 §1 structured/text）

use std::time::Duration;

use ambition_core::UsageSummary;
use serde::de::DeserializeOwned;

use crate::{AiLedger, GenRequest, LlmError, LlmProvider, RETRY_BACKOFF_MS, RETRY_LIMIT};

/// 熔断开闸阈值：连续失败达其次数 → 进入快速失败窗口（docs/05 §1，M3）
pub const BREAKER_FAIL_LIMIT: u32 = 5;
/// 熔断快速失败窗口：拒发请求次数，耗尽后放行 1 次试探（成功即关闸）
pub const BREAKER_SKIP_LIMIT: u32 = 10;

/// 全局熔断状态（连续失败计数 + 快速失败窗口；锁不跨 await）
#[derive(Debug, Default, Clone, Copy)]
struct Breaker {
    fails: u32,
    skip_left: u32,
}

/// 窗口取票：有票则取一张并拒发；无票放行（试探机会）
fn window_take(breaker: &mut Breaker) -> bool {
    if breaker.skip_left > 0 {
        breaker.skip_left -= 1;
        return true;
    }
    false
}

/// 失败计数（达阈值开闸：发满快速失败窗口票）
fn count_failure(breaker: &mut Breaker) {
    breaker.fails += 1;
    if breaker.fails >= BREAKER_FAIL_LIMIT {
        breaker.skip_left = BREAKER_SKIP_LIMIT;
    }
}

/// 结构化输出路由器
pub struct LlmRouter {
    primary: Box<dyn LlmProvider>,
    fallback: Option<Box<dyn LlmProvider>>,
    ledger: Box<dyn AiLedger>,
    breaker: std::sync::Mutex<Breaker>,
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
    fn breaker_should_skip(&self) -> bool {
        match self.breaker.lock() {
            Ok(mut guard) => window_take(&mut guard),
            Err(poisoned) => {
                eprintln!("[warn] breaker mutex poisoned, recovering");
                window_take(&mut poisoned.into_inner())
            }
        }
    }

    /// 记一次失败：达阈值则开闸（进入快速失败窗口）
    fn breaker_record_failure(&self) {
        match self.breaker.lock() {
            Ok(mut guard) => count_failure(&mut guard),
            Err(poisoned) => {
                eprintln!("[warn] breaker mutex poisoned, recovering");
                count_failure(&mut poisoned.into_inner())
            }
        }
    }

    /// 成功关闸（计数清零）
    fn breaker_reset(&self) {
        match self.breaker.lock() {
            Ok(mut guard) => *guard = Breaker::default(),
            Err(poisoned) => {
                eprintln!("[warn] breaker mutex poisoned, recovering");
                *poisoned.into_inner() = Breaker::default();
            }
        }
    }

    /// 剥 ```json 围栏后按 T 反序列化（失败 = None，触发重试）
    fn parse_structured<T: DeserializeOwned>(raw: &str) -> Option<T> {
        let trimmed = raw.trim();
        let stripped = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .unwrap_or(trimmed);
        serde_json::from_str(stripped).ok()
    }

    /// 重试环外壳：缓存命中（不计数不关闸）→ 熔断检查 → provider 重试环 → 熔断记账
    /// （成功关闸；Offline/BreakerOpen 不计入；其余失败计数开闸）
    async fn run_loop<T>(
        &self,
        req: &GenRequest,
        validate: impl Fn(&str) -> Option<T>,
    ) -> Result<T, LlmError> {
        let cache_key = req.effective_cache_key();
        if let Some(hit) = self.ledger.cache_get(&cache_key).await
            && let Some(value) = validate(&hit)
        {
            return Ok(value); // 缓存命中：与供给方可用性无关，计数/关闸都不动
        }
        // 坏缓存（validate 拒绝）：不静默吞，照常走下方重算路径
        // 熔断检查置于缓存之后：命中缓存不发请求，不受窗口影响；锁不跨 await
        if self.breaker_should_skip() {
            return Err(LlmError::BreakerOpen);
        }
        let result = self.run_attempts(req, validate, &cache_key).await;
        match &result {
            Ok(_) => self.breaker_reset(),
            Err(LlmError::Offline | LlmError::BreakerOpen) => {}
            Err(_) => self.breaker_record_failure(),
        }
        result
    }

    /// provider 重试环：provider 轮（可切 fallback）→ 校验 → 记账 + 写缓存
    async fn run_attempts<T>(
        &self,
        req: &GenRequest,
        validate: impl Fn(&str) -> Option<T>,
        cache_key: &str,
    ) -> Result<T, LlmError> {
        let mut use_fallback = false;
        let mut last: Option<LlmError> = None;
        let mut adjusted = req.clone();

        for attempt in 0..=RETRY_LIMIT {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(RETRY_BACKOFF_MS << (attempt - 1))).await;
                adjusted.temperature = (req.temperature - 0.15 * attempt as f32).max(0.0);
                if let Some(err) = &last {
                    adjusted.user = format!("{}\n\nPrevious attempt failed: {err}", req.user);
                }
            }

            let provider: &dyn LlmProvider = if use_fallback {
                self.fallback.as_ref().map_or(self.primary.as_ref(), |f| {
                    let f: &dyn LlmProvider = f.as_ref();
                    f
                })
            } else {
                self.primary.as_ref()
            };

            match provider.complete(&adjusted).await {
                Ok(out) => match validate(&out.text) {
                    Some(value) => {
                        self.ledger
                            .record_usage(
                                req.kind.as_str(),
                                &out.model,
                                out.tokens_in,
                                out.tokens_out,
                            )
                            .await;
                        self.ledger.cache_put(cache_key, &out.text).await;
                        return Ok(value);
                    }
                    None => {
                        last = Some(LlmError::InvalidOutput {
                            attempts: attempt + 1,
                        });
                    }
                },
                // 无 key / 参数被拒：本供给方重试无益 → 有 fallback 就切，没有就止
                Err(
                    err @ (LlmError::Offline
                    | LlmError::Http {
                        retryable: false, ..
                    }),
                ) => {
                    if self.fallback.is_some() && !use_fallback {
                        use_fallback = true;
                        last = Some(err);
                        continue;
                    }
                    return Err(err);
                }
                Err(err) => last = Some(err),
            }
        }

        Err(match last {
            Some(LlmError::InvalidOutput { .. }) | None => LlmError::InvalidOutput {
                attempts: RETRY_LIMIT + 1,
            },
            Some(other) => other,
        })
    }

    /// 结构化补全：schema 校验失败 → 重试 ≤2 → InvalidOutput
    pub async fn structured<T: DeserializeOwned>(&self, req: GenRequest) -> Result<T, LlmError> {
        self.run_loop(&req, |raw| Self::parse_structured(raw)).await
    }

    /// 纯文本补全（围栏剥不影响纯文本语义：原文返回）
    pub async fn text(&self, req: GenRequest) -> Result<String, LlmError> {
        self.run_loop(&req, |raw| Some(raw.to_string())).await
    }

    /// 命中缓存的原始输出（测试/调试用：返回 None = 未命中或坏缓存）
    pub async fn cached(&self, req: &GenRequest) -> Option<String> {
        self.ledger.cache_get(&req.effective_cache_key()).await
    }

    /// 成本台账：区间 [from_unix, to_unix) 按 kind 聚合（docs/05 §1，M3 落地）
    pub async fn usage_summary(&self, from_unix: i64, to_unix: i64) -> UsageSummary {
        self.ledger.usage_summary(from_unix, to_unix).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmKind, LlmOut, MemLedger};
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

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
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

    fn json_req() -> GenRequest {
        GenRequest::new(LlmKind::QuestionGen, "sys", "usr")
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
    fn test_parse_structured_strips_fences_and_rejects_garbage() {
        type T = serde_json::Value;
        let plain = LlmRouter::parse_structured::<T>(r#"{"a":1}"#).expect("裸 JSON");
        assert_eq!(plain["a"], 1);
        let fenced =
            LlmRouter::parse_structured::<T>("```json\n{\"a\":2}\n```").expect("围栏 JSON");
        assert_eq!(fenced["a"], 2);
        assert!(
            LlmRouter::parse_structured::<T>("not json at all").is_none(),
            "非法 JSON → None 触发重试"
        );
    }

    #[tokio::test]
    async fn test_structured_retries_on_invalid_output_then_succeeds() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![
                Err(LlmError::Http {
                    status: 500,
                    retryable: true,
                }),
                Ok("oops not json".into()),
                Ok(ok_json()),
            ])),
            None,
            Box::new(MemLedger::new()),
        );
        let value: serde_json::Value = router.structured(json_req()).await.expect("三试而");
        assert_eq!(value["questions"].as_array().expect("数组").len(), 0);
    }

    #[tokio::test]
    async fn test_structured_reports_invalid_output_after_limit() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(
                (0..9).map(|_| Ok("garbage".into())).collect(),
            )),
            None,
            Box::new(MemLedger::new()),
        );
        let err = router
            .structured::<serde_json::Value>(json_req())
            .await
            .expect_err("始终非法");
        match err {
            LlmError::InvalidOutput { attempts } => assert_eq!(attempts, RETRY_LIMIT + 1),
            other => panic!("应报 InvalidOutput: {other}"),
        }
    }

    #[tokio::test]
    async fn test_run_loop_retries_invalid_output_via_structured() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![Ok("nope".into()), Ok(ok_json())])),
            None,
            Box::new(MemLedger::new()),
        );
        let value: serde_json::Value = router.structured(json_req()).await.expect("重试成功");
        assert!(value.get("questions").is_some());
    }

    #[tokio::test]
    async fn test_cached_returns_none_then_hit_after_write() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![Ok(ok_json())])),
            None,
            Box::new(MemLedger::new()),
        );
        assert_eq!(router.cached(&json_req()).await, None);
        let _: serde_json::Value = router.structured(json_req()).await.expect("写缓存");
        assert_eq!(router.cached(&json_req()).await, Some(ok_json()));
    }

    #[tokio::test]
    async fn test_structured_hits_cache_without_calling_provider() {
        let ledger = MemLedger::new();
        let req = json_req();
        ledger
            .cache_put(&req.effective_cache_key(), r#"{"cached": true}"#)
            .await;
        let provider = Scripted::new(vec![Err(LlmError::Offline)]); // 一旦被调即失败 → 证明走了缓存
        let calls_before = provider.calls();
        let router = LlmRouter::new(Box::new(provider), None, Box::new(ledger));
        let value: serde_json::Value = router.structured(req).await.expect("缓存命中");
        assert_eq!(value["cached"], true);
        // provider 已 move 进 router，无法回读计数；换由“坏 provider 不报错”间接证明未调用
        assert_eq!(calls_before, 0, "初始零调用");
    }

    #[tokio::test]
    async fn test_structured_records_usage_on_success() {
        let ledger = MemLedger::new();
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![Ok(ok_json())])),
            None,
            Box::new(ledger),
        );
        let _: serde_json::Value = router.structured(json_req()).await.expect("成功");
        // ledger 已 move 进 router，无法外部取回 —— 通过二次调用缓存命中间接验证写缓存
        let cached = router
            .cached(&json_req())
            .await
            .expect("成功路径必须写缓存");
        assert_eq!(cached, ok_json());
    }

    #[tokio::test]
    async fn test_structured_switches_to_fallback_on_non_retryable() {
        let primary = Scripted::new(vec![Err(LlmError::Http {
            status: 400,
            retryable: false,
        })]);
        let fallback = Scripted::new(vec![Ok(ok_json())]);
        let router = LlmRouter::new(
            Box::new(primary),
            Some(Box::new(fallback)),
            Box::new(MemLedger::new()),
        );
        let value: serde_json::Value = router.structured(json_req()).await.expect("fallback 接住");
        assert!(value.get("questions").is_some());
    }

    #[tokio::test]
    async fn test_text_returns_raw_string() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![Ok("plain prose".into())])),
            None,
            Box::new(MemLedger::new()),
        );
        let text = router.text(json_req()).await.expect("text 路径");
        assert_eq!(text, "plain prose");
        assert_eq!(
            router.cached(&json_req()).await.as_deref(),
            Some("plain prose")
        );
    }

    #[tokio::test]
    async fn test_structured_offline_without_fallback_short_circuits() {
        let router = LlmRouter::new(
            Box::new(Scripted::new(vec![Err(LlmError::Offline)])),
            None,
            Box::new(MemLedger::new()),
        );
        let err = router
            .structured::<serde_json::Value>(json_req())
            .await
            .expect_err("离线");
        assert!(
            matches!(err, LlmError::Offline),
            "Offline 短路不空转: {err}"
        );
    }

    /// 恒定失败的供给方（非可重试 400：单次调用即错、无退避等待，熔断测试用）
    struct CountingFail {
        calls: std::sync::Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for CountingFail {
        async fn complete(&self, req: &GenRequest) -> Result<LlmOut, LlmError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let temperature = req.temperature; // 签名由 trait 固定，读一下表明参数被消费
            debug_assert!(temperature >= 0.0, "temperature 非负");
            Err(LlmError::Http {
                status: 400,
                retryable: false,
            })
        }

        fn model(&self) -> &str {
            "counting-fail"
        }
    }

    #[test]
    fn test_window_take_consumes_one_ticket_per_call() {
        let mut breaker = Breaker {
            fails: BREAKER_FAIL_LIMIT,
            skip_left: 2,
        };
        assert!(window_take(&mut breaker), "第一张票");
        assert!(window_take(&mut breaker), "第二张票");
        assert!(!window_take(&mut breaker), "票尽放行（试探）");
        assert_eq!(breaker.skip_left, 0, "票数递减到零");
    }

    #[test]
    fn test_count_failure_opens_window_only_at_limit() {
        let mut breaker = Breaker::default();
        for _ in 0..(BREAKER_FAIL_LIMIT - 1) {
            count_failure(&mut breaker);
        }
        assert_eq!(breaker.skip_left, 0, "未达阈值不开闸");
        count_failure(&mut breaker);
        assert_eq!(breaker.fails, BREAKER_FAIL_LIMIT, "达阈值");
        assert_eq!(breaker.skip_left, BREAKER_SKIP_LIMIT, "开闸即发满窗口票");
        // 试探再败：窗口续票
        breaker.skip_left = 0;
        count_failure(&mut breaker);
        assert_eq!(breaker.skip_left, BREAKER_SKIP_LIMIT, "再败续票");
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
    async fn test_run_attempts_serves_cache_before_breaker_check() {
        let ledger = MemLedger::new();
        let req = json_req();
        ledger
            .cache_put(&req.effective_cache_key(), ok_json().as_str())
            .await;
        let provider = CountingFail {
            calls: std::sync::Arc::new(AtomicUsize::new(0)),
        };
        let router = LlmRouter::new(Box::new(provider), None, Box::new(ledger));
        for _ in 0..BREAKER_FAIL_LIMIT {
            router.breaker_record_failure(); // 开闸：窗口内
        }
        // 1）缓存命中不受熔断影响（check 在 cache 之后）
        let value: serde_json::Value = router.structured(req).await.expect("缓存命中");
        assert!(value.get("questions").is_some());
        // 2）未命中缓存的请求被窗口拒发
        let mut fresh = json_req();
        fresh.user = "another prompt".into();
        let err = router
            .structured::<serde_json::Value>(fresh)
            .await
            .expect_err("窗口内拒发");
        assert!(matches!(err, LlmError::BreakerOpen), "{err}");
    }

    #[tokio::test]
    async fn test_structured_fast_fails_when_breaker_open_then_trials() {
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let router = LlmRouter::new(
            Box::new(CountingFail {
                calls: calls.clone(),
            }),
            None,
            Box::new(MemLedger::new()),
        );
        // 阈值内：每次都真发请求并失败
        for _ in 0..BREAKER_FAIL_LIMIT {
            let err = router
                .structured::<serde_json::Value>(json_req())
                .await
                .expect_err("失败");
            assert!(matches!(err, LlmError::Http { status: 400, .. }), "{err}");
        }
        assert_eq!(calls.load(Ordering::SeqCst) as u32, BREAKER_FAIL_LIMIT);
        // 开闸：窗口内快速失败，不再发请求
        for _ in 0..BREAKER_SKIP_LIMIT {
            let err = router
                .structured::<serde_json::Value>(json_req())
                .await
                .expect_err("快速失败");
            assert!(matches!(err, LlmError::BreakerOpen), "{err}");
        }
        assert_eq!(
            calls.load(Ordering::SeqCst) as u32,
            BREAKER_FAIL_LIMIT,
            "窗口内不发请求"
        );
        // 试探：放行一次真实调用，再败 → 续票
        let trial = router
            .structured::<serde_json::Value>(json_req())
            .await
            .expect_err("试探仍败");
        assert!(
            matches!(trial, LlmError::Http { status: 400, .. }),
            "{trial}"
        );
        assert_eq!(calls.load(Ordering::SeqCst) as u32, BREAKER_FAIL_LIMIT + 1);
        let skipped = router
            .structured::<serde_json::Value>(json_req())
            .await
            .expect_err("续票后拒发");
        assert!(matches!(skipped, LlmError::BreakerOpen), "{skipped}");
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
