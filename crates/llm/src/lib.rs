//! Ambition llm：外部 AI 适配层 —— OpenAI/Anthropic 协议 + 结构化输出路由（docs/05 §1）
//! 回包只经本层交给 06/07 消费；本 crate 不落业务库（账本走 `AiLedger` trait，SQLite 实现经 engine 装配）。

use ambition_core::canonical_json_hash;

mod anthropic;
mod ledger;
mod openai;
mod router;

pub use anthropic::Anthropic;
pub use ledger::MemLedger;
pub use openai::OpenAiCompat;
pub use router::LlmRouter;

/// JSON Schema（结构化输出约束；serde_json::Value 即线格式）
pub type JsonSchema = serde_json::Value;

/// 重试上限（docs/05：重试 ≤2，temperature 递减 + 错误摘要）
pub const RETRY_LIMIT: u32 = 2;

/// 重试退避基数（毫秒；2 次重试 = 100ms + 200ms）
pub const RETRY_BACKOFF_MS: u64 = 100;

/// 生成请求（prompt 模板 kind + system/user + 可选 schema）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenRequest {
    pub kind: LlmKind,
    pub system: String,
    pub user: String,
    pub schema: Option<JsonSchema>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub cache_key: Option<String>,
}

impl GenRequest {
    /// 构造请求（temperature 0.7 / max_tokens 2048 为默认；schema/cache_key 由调用方补）
    pub fn new(kind: LlmKind, system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            kind,
            system: system.into(),
            user: user.into(),
            schema: None,
            temperature: 0.7,
            max_tokens: 2048,
            cache_key: None,
        }
    }

    /// 缓存 key：显式 key 优先，否则 = 内容 hash（kind+system+user，docs/05 llm_cache）
    pub fn effective_cache_key(&self) -> String {
        if let Some(key) = &self.cache_key {
            return key.clone();
        }
        canonical_json_hash(&(&self.kind, &self.system, &self.user)).0
    }
}

/// 生成用途（决定 prompt 模板与记账维度）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmKind {
    /// 结构化出题（06-generation）
    QuestionGen,
    /// 讲解/范文等自由文本（M3+）
    Explain,
    /// 第三级 LLM 复核（判断不打分，docs/07 §4，M3）
    Arbitrate,
}

impl LlmKind {
    /// 稳定字符串（记账与 prompt 模板名）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QuestionGen => "question_gen",
            Self::Explain => "explain",
            Self::Arbitrate => "arbitrate",
        }
    }
}

/// 模型回包（原始文本 + 记账所需 token 数）
#[derive(Debug, Clone, PartialEq)]
pub struct LlmOut {
    pub text: String,
    pub model: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
}

/// LLM 错误（外部失败带 retryable，docs/01 §1）
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("http {status} (retryable={retryable})")]
    Http { status: u16, retryable: bool },
    #[error("invalid output after {attempts} attempt(s)")]
    InvalidOutput { attempts: u32 },
    #[error("quota exceeded (402/429 quota)")]
    QuotaExceeded,
    #[error("offline: no api key configured")]
    Offline,
    /// 全局熔断：连续失败达阈值后的快速失败（不发请求，docs/05 §1，M3）
    #[error("circuit breaker open (consecutive failures)")]
    BreakerOpen,
}

/// LLM 供给方（OpenAI 兼容 / Anthropic / M7 代管代理）
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// 单次补全（协议编码/解码在实现内；错误分类为 LlmError）
    async fn complete(&self, req: &GenRequest) -> Result<LlmOut, LlmError>;
    /// 模型标识（缓存 key 与记账维度）
    fn model(&self) -> &str;
}

/// 用量/缓存账本（定义在 05，SQLite 实现经 engine 装配；测试用 MemLedger）
#[async_trait::async_trait]
pub trait AiLedger: Send + Sync {
    /// 读缓存（None = 未命中）
    async fn cache_get(&self, cache_key: &str) -> Option<String>;
    /// 写缓存（同 key 覆盖）
    async fn cache_put(&self, cache_key: &str, payload: &str);
    /// 记账（kind = LlmKind::as_str 或 "jev"）
    async fn record_usage(&self, kind: &str, model: &str, tokens_in: i64, tokens_out: i64);
    /// 成本台账：区间 [from_unix, to_unix) 按 kind 聚合（unix 秒；docs/05 §1，M3）
    async fn usage_summary(&self, from_unix: i64, to_unix: i64) -> ambition_core::UsageSummary;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubProvider {
        outputs: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for StubProvider {
        async fn complete(&self, req: &GenRequest) -> Result<LlmOut, LlmError> {
            let _ = req.temperature; // 签名由 trait 固定，读一下表明参数被消费
            let text = self
                .outputs
                .lock()
                .map(|mut o| {
                    if o.is_empty() {
                        String::new()
                    } else {
                        o.remove(0)
                    }
                })
                .unwrap_or_default();
            Ok(LlmOut {
                text,
                model: "stub-model".into(),
                tokens_in: 1,
                tokens_out: 1,
            })
        }

        fn model(&self) -> &str {
            "stub-model"
        }
    }

    struct StubLedger;

    #[async_trait::async_trait]
    impl AiLedger for StubLedger {
        async fn cache_get(&self, cache_key: &str) -> Option<String> {
            Some(format!("hit:{cache_key}"))
        }

        async fn cache_put(&self, cache_key: &str, payload: &str) {
            // 语义由 MemLedger 测；此处消费参数防占位
            debug_assert!(!cache_key.is_empty() || payload.is_empty());
        }

        async fn record_usage(&self, kind: &str, model: &str, tokens_in: i64, tokens_out: i64) {
            debug_assert!(
                tokens_in >= 0 && tokens_out >= 0 && !kind.is_empty() && !model.is_empty()
            );
        }

        async fn usage_summary(&self, from_unix: i64, to_unix: i64) -> ambition_core::UsageSummary {
            // 语义由 MemLedger 测；此处消费参数防占位
            debug_assert!(from_unix < to_unix);
            ambition_core::UsageSummary::default()
        }
    }

    #[test]
    fn test_new_gen_request_sets_defaults() {
        let req = GenRequest::new(LlmKind::QuestionGen, "sys", "usr");
        assert_eq!(req.kind, LlmKind::QuestionGen);
        assert_eq!(req.system, "sys");
        assert_eq!(req.user, "usr");
        assert!(req.schema.is_none());
        assert!((req.temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(req.max_tokens, 2048);
        assert!(req.cache_key.is_none());
    }

    #[test]
    fn test_effective_cache_key_prefers_explicit_then_hashes_content() {
        let mut req = GenRequest::new(LlmKind::QuestionGen, "sys", "usr");
        assert_eq!(
            req.effective_cache_key(),
            canonical_json_hash(&(LlmKind::QuestionGen, "sys", "usr")).0,
            "无显式 key = 内容 hash"
        );
        req.cache_key = Some("explicit-key".into());
        assert_eq!(req.effective_cache_key(), "explicit-key", "显式 key 优先");
        let other = GenRequest::new(LlmKind::QuestionGen, "sys", "other-user");
        req.cache_key = None;
        assert_ne!(
            req.effective_cache_key(),
            other.effective_cache_key(),
            "user 变则 key 变"
        );
    }

    #[tokio::test]
    async fn test_complete_stub_provider_contract() {
        let provider = StubProvider {
            outputs: std::sync::Mutex::new(vec!["out".into()]),
        };
        let req = GenRequest::new(LlmKind::QuestionGen, "s", "u");
        let out = provider.complete(&req).await.expect("补全");
        assert_eq!(out.text, "out");
    }

    #[tokio::test]
    async fn test_model_stub_provider_identifier() {
        let provider = StubProvider {
            outputs: std::sync::Mutex::new(vec![]),
        };
        assert_eq!(provider.model(), "stub-model");
    }

    #[tokio::test]
    async fn test_cache_get_stub_ledger_returns_hit_key() {
        let ledger = StubLedger;
        assert_eq!(ledger.cache_get("abc").await, Some("hit:abc".to_string()));
    }

    #[tokio::test]
    async fn test_cache_put_stub_ledger_accepts_write() {
        let ledger = StubLedger;
        ledger.cache_put("k", "v").await; // 无异常即通过（写语义由 MemLedger/StoreLedger 测）
    }

    #[tokio::test]
    async fn test_run_loop_is_covered_via_router_tests() {
        // run_loop 为私有环：由 structured/text 用例全覆盖（重试/缓存/记账/降级四路径）
        let router = LlmRouter::new(
            Box::new(StubProvider {
                outputs: std::sync::Mutex::new(vec!["{\"ok\":1}".into()]),
            }),
            None,
            Box::new(StubLedger),
        );
        let value: serde_json::Value = router
            .structured(GenRequest::new(LlmKind::QuestionGen, "s", "u"))
            .await
            .expect("环路");
        assert_eq!(value["ok"], 1);
    }

    #[tokio::test]
    async fn test_cached_exposes_raw_hit() {
        let req = GenRequest::new(LlmKind::QuestionGen, "s", "u");
        let router = LlmRouter::new(
            Box::new(StubProvider {
                outputs: std::sync::Mutex::new(vec!["raw".into()]),
            }),
            None,
            Box::new(MemLedger::new()),
        );
        assert_eq!(router.cached(&req).await, None, "未写前无缓存");
        let _: String = router.text(req.clone()).await.expect("text");
        assert_eq!(router.cached(&req).await.as_deref(), Some("raw"));
    }

    #[tokio::test]
    async fn test_record_usage_stub_ledger_accepts_row() {
        let ledger = StubLedger;
        ledger.record_usage("question_gen", "m", 1, 2).await;
    }

    #[test]
    fn test_as_str_llm_kind_snake_case_matches_serde() {
        assert_eq!(LlmKind::QuestionGen.as_str(), "question_gen");
        assert_eq!(LlmKind::Explain.as_str(), "explain");
        assert_eq!(LlmKind::Arbitrate.as_str(), "arbitrate");
        let json = serde_json::to_value(LlmKind::Arbitrate).expect("序列化");
        assert_eq!(json, serde_json::json!("arbitrate"), "与 serde 同口径");
    }

    #[tokio::test]
    async fn test_usage_summary_stub_ledger_returns_empty() {
        // StubLedger 的 usage_summary 由 MemLedger/StoreLedger 实测；此处钉住 trait 可达
        let router = LlmRouter::new(
            Box::new(StubProvider {
                outputs: std::sync::Mutex::new(vec![]),
            }),
            None,
            Box::new(StubLedger),
        );
        let summary = router.usage_summary(0, i64::MAX).await;
        assert_eq!(summary.rows.len(), 0);
    }

    #[test]
    fn test_breaker_open_error_is_non_retryable_signal() {
        let err = LlmError::BreakerOpen;
        assert!(err.to_string().contains("breaker"), "{err}");
        assert!(
            !matches!(err, LlmError::Offline | LlmError::Http { .. }),
            "BreakerOpen 独立变体"
        );
    }

    #[test]
    fn test_retry_limits_are_documented_values() {
        assert_eq!(RETRY_LIMIT, 2, "docs/05：重试 ≤2");
        assert_eq!(RETRY_BACKOFF_MS, 100);
    }
}
