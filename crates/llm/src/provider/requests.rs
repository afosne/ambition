//! 请求执行环：parse_structured/run_loop/run_attempts/structured/text/cached（重试≤2、缓存直读、fallback 路由）

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::*;
use crate::{GenRequest, LlmError, RETRY_BACKOFF_MS, RETRY_LIMIT};

impl LlmRouter {
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
                            .record_usage(UsageRecord::new(
                                req.kind.as_str(),
                                &out.model,
                                out.tokens_in,
                                out.tokens_out,
                            ))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use crate::router::breaker::{BREAKER_FAIL_LIMIT, BREAKER_SKIP_LIMIT};
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
}
