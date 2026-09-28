//! Jev 客户端：JevClient/evaluate/evaluate_many + 退避/超时/记账（docs/05 §5）
//! 本域含错误分类子组件（`client::error`，docs/01 §1 细分功能目录）。

pub mod error;

use crate::*;

/// 默认端点（docs/05 §2）
pub const DEFAULT_JEV_BASE: &str = "https://api.typesafe.ai/v1";

/// 默认模型
pub const DEFAULT_JEV_MODEL: &str = "jev-latest";

/// 重试上限（docs/05：429/529/5xx 指数退避 ≤3）
pub const JEV_RETRY_LIMIT: u32 = 3;

/// 退避基数（毫秒）
pub const JEV_BACKOFF_MS: u64 = 200;

/// 单请求超时（docs/05：15s）
pub const JEV_TIMEOUT_SECS: u64 = 15;

/// fan-out 并发池上限（docs/05 §2：并发池 8，M3）
pub const JEV_CONCURRENCY: usize = 8;

/// Jev 客户端（薄 HTTP；无 Rust SDK，docs/05 §3.5）
pub struct JevClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    ledger: Option<std::sync::Arc<dyn AiLedger>>,
}

impl JevClient {
    /// 构造（base 默认 typesafe v1；ledger 可选：None = 不记账）
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_options(DEFAULT_JEV_BASE, DEFAULT_JEV_MODEL, api_key, None)
    }

    /// 全参构造（测试指 base 到 wiremock；ledger 可选）
    pub fn with_options(
        base_url: &str,
        model: &str,
        api_key: Option<String>,
        ledger: Option<std::sync::Arc<dyn AiLedger>>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(JEV_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model: model.to_string(),
            ledger,
        }
    }

    /// 一次请求 fan-out 全部问题（重试 ≤3：429/529/5xx 指数退避；记账 kind=jev）
    pub async fn evaluate(
        &self,
        state: JevState,
        questions: &JevQuestions,
    ) -> Result<JevAnswers, JevError> {
        let api_key = self.api_key.as_deref().ok_or(JevError::Offline)?;
        let url = format!("{}/systemone", self.base_url);
        let body = encode_request(&state, &self.model, questions);

        let mut last: Option<JevError> = None;
        for attempt in 0..=JEV_RETRY_LIMIT {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(JEV_BACKOFF_MS << (attempt - 1))).await;
            }
            let resp = self
                .http
                .post(&url)
                .bearer_auth(api_key)
                .json(&body)
                .send()
                .await
                .map_err(|_| JevError::Http {
                    status: 0,
                    retryable: true,
                })?;
            let status = resp.status().as_u16();
            if !(200..300).contains(&status) {
                let err = classify_jev_status(status);
                match &err {
                    JevError::Http {
                        retryable: true, ..
                    } => {
                        last = Some(err);
                        continue;
                    }
                    other => return Err(other.clone()),
                }
            }
            let payload = resp
                .json::<serde_json::Value>()
                .await
                .map_err(|_| JevError::Invalid("response not json".into()))?;
            let answers = decode_response(&payload)?;
            self.record_usage(&payload).await;
            return Ok(answers);
        }
        Err(last.unwrap_or(JevError::Http {
            status: 502,
            retryable: true,
        }))
    }

    /// 批量 fan-out（并发池 `JEV_CONCURRENCY`=8、**保序**、单题失败不中断，docs/05 §2，M3）
    pub async fn evaluate_many(&self, jobs: Vec<JevJob>) -> Vec<Result<JevAnswers, JevError>> {
        use futures::StreamExt as _;
        futures::stream::iter(jobs)
            .map(|job| async move { self.evaluate(job.state, &job.questions).await })
            .buffered(JEV_CONCURRENCY)
            .collect()
            .await
    }

    /// 记账（旁路：失败只升级为返回值里的信号——evaluate 成功路径不因它失败）
    async fn record_usage(&self, payload: &serde_json::Value) {
        let Some(ledger) = &self.ledger else { return };
        let usage = payload.get("usage");
        let tokens_in = usage
            .and_then(|u| u.get("input_tokens"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let tokens_out = usage
            .and_then(|u| u.get("output_tokens"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        ledger
            .record_usage("jev", &self.model, tokens_in, tokens_out)
            .await;
    }

    /// 模型标识（记账维度）
    pub fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_llm::MemLedger;
    use std::sync::Arc;
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_questions() -> JevQuestions {
        let mut qs = JevQuestions::new();
        qs.insert(
            "unique_answer".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "Is exactly one option correct?".into(),
            }),
        );
        qs.insert(
            "natural".into(),
            JevQuestion::Score(ScoreQ {
                instructions: "How natural is the sentence?".into(),
                levels: vec!["unnatural".into(), "ok".into(), "natural".into()],
            }),
        );
        qs.insert(
            "error_cause".into(),
            JevQuestion::Choice(ChoiceQ {
                instructions: "Classify the mistake.".into(),
                criteria: [("word_sense".to_string(), "wrong sense".to_string())]
                    .into_iter()
                    .collect(),
            }),
        );
        qs
    }

    fn ok_body() -> serde_json::Value {
        serde_json::json!({
            "model": "jev-1.13.0",
            "answers": {
                "unique_answer": {"type": "noul", "noul": 0.95},
                "natural": {"type": "score", "score": 1.9, "confidence": 0.9,
                            "legend": {"0":"unnatural","1":"ok","2":"natural"},
                            "probabilities": {"0":0.0,"1":0.05,"2":0.95}},
                "error_cause": {"type": "choice", "choice": "word_sense",
                                "probabilities": {"word_sense": 0.9}, "confidence": 0.88}
            },
            "usage": {"input_tokens": 300, "output_tokens": 25}
        })
    }

    fn fanout_job(marker: &str) -> JevJob {
        JevJob {
            state: JevState(serde_json::json!({ "stem": marker })),
            questions: sample_questions(),
        }
    }

    #[tokio::test]
    async fn test_evaluate_wire_contract_posts_systemone_with_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_string_contains("\"jev-latest\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
            .expect(1)
            .mount(&server)
            .await;

        let client = JevClient::with_options(
            &format!("{}/v1", server.uri()),
            DEFAULT_JEV_MODEL,
            Some("test-key".into()),
            None,
        );
        let answers = client
            .evaluate(
                JevState(serde_json::json!("English only")),
                &sample_questions(),
            )
            .await
            .expect("evaluate 成功");
        assert_eq!(answers.len(), 3);
        assert_eq!(client.model(), "jev-latest");
    }

    #[tokio::test]
    async fn test_evaluate_retries_429_then_succeeds_and_records_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
            .mount(&server)
            .await;

        let ledger = Arc::new(MemLedger::new());
        let client = JevClient::with_options(
            &format!("{}/v1", server.uri()),
            DEFAULT_JEV_MODEL,
            Some("k".into()),
            Some(ledger.clone()),
        );
        client
            .evaluate(JevState(serde_json::json!("s")), &sample_questions())
            .await
            .expect("429 后重试成功");
        let rows = ledger.take_usage();
        assert_eq!(rows.len(), 1, "记账 kind=jev");
        assert_eq!(rows[0].kind, "jev");
        assert_eq!(rows[0].tokens_in, 300);
        assert_eq!(rows[0].tokens_out, 25);
    }

    #[tokio::test]
    async fn test_evaluate_maps_401_to_non_retryable_and_keyless_to_offline() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let client = JevClient::with_options(
            &format!("{}/v1", server.uri()),
            DEFAULT_JEV_MODEL,
            Some("bad".into()),
            None,
        );
        let err = client
            .evaluate(JevState(serde_json::json!("s")), &sample_questions())
            .await
            .expect_err("401");
        assert!(
            matches!(
                err,
                JevError::Http {
                    status: 401,
                    retryable: false
                }
            ),
            "401 不重试: {err}"
        );

        let keyless = JevClient::with_options(
            &format!("{}/v1", server.uri()),
            DEFAULT_JEV_MODEL,
            None,
            None,
        );
        let err2 = keyless
            .evaluate(JevState(serde_json::json!("s")), &sample_questions())
            .await
            .expect_err("无 key");
        assert!(
            matches!(err2, JevError::Offline),
            "无 key → Offline: {err2}"
        );
    }

    #[test]
    fn test_new_client_uses_documented_defaults() {
        let client = JevClient::new(Some("k".into()));
        assert_eq!(client.base_url, DEFAULT_JEV_BASE);
        assert_eq!(client.model, DEFAULT_JEV_MODEL);
        assert!(client.ledger.is_none());
        let keyless = JevClient::new(None);
        assert!(keyless.api_key.is_none());
    }

    #[test]
    fn test_with_options_overrides_base_model_and_ledger() {
        let ledger = Arc::new(MemLedger::new());
        let client = JevClient::with_options(
            "http://localhost:9/v2/",
            "jev-pin",
            Some("k".into()),
            Some(ledger),
        );
        assert_eq!(client.base_url, "http://localhost:9/v2", "尾斜杠修剪");
        assert_eq!(client.model(), "jev-pin");
        assert!(client.ledger.is_some());
    }

    #[tokio::test]
    async fn test_record_usage_writes_jev_row_when_ledger_present() {
        let ledger = Arc::new(MemLedger::new());
        let client = JevClient::with_options(
            "http://localhost:9",
            DEFAULT_JEV_MODEL,
            Some("k".into()),
            Some(ledger.clone()),
        );
        let payload = ok_body();
        client.record_usage(&payload).await;
        let rows = ledger.take_usage();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "jev");
        // 无账本路径：不 panic
        let bare = JevClient::with_options(
            "http://localhost:9",
            DEFAULT_JEV_MODEL,
            Some("k".into()),
            None,
        );
        bare.record_usage(&payload).await;
    }

    #[test]
    fn test_model_reports_configured_identifier() {
        let client = JevClient::with_options("http://localhost:9", "jev-x", None, None);
        assert_eq!(client.model(), "jev-x");
    }

    #[tokio::test]
    async fn test_evaluate_many_isolates_failure_and_preserves_order() {
        let server = MockServer::start().await;
        // job-a → 422（不可重试，快速失败）；job-b → 200
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(body_string_contains("job-a"))
            .respond_with(ResponseTemplate::new(422))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(body_string_contains("job-b"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
            .mount(&server)
            .await;

        let client = JevClient::with_options(
            &format!("{}/v1", server.uri()),
            DEFAULT_JEV_MODEL,
            Some("k".into()),
            None,
        );
        let results = client
            .evaluate_many(vec![fanout_job("job-a"), fanout_job("job-b")])
            .await;
        assert_eq!(results.len(), 2, "单题失败不中断，槽数不变");
        assert!(
            matches!(
                &results[0],
                Err(JevError::Http {
                    status: 422,
                    retryable: false
                })
            ),
            "槽位 0 = 失败且保序: {:?}",
            results[0]
        );
        assert!(
            results[1].as_ref().is_ok_and(|a| a.len() == 3),
            "槽位 1 = 成功（失败不拖垮邻槽）"
        );
    }

    #[tokio::test]
    async fn test_evaluate_many_handles_batch_beyond_pool_capacity() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
            .mount(&server)
            .await;
        let client = JevClient::with_options(
            &format!("{}/v1", server.uri()),
            DEFAULT_JEV_MODEL,
            Some("k".into()),
            None,
        );
        let jobs: Vec<JevJob> = (0..JEV_CONCURRENCY * 2 + 2)
            .map(|i| fanout_job(&format!("pool-{i}")))
            .collect();
        let results = client.evaluate_many(jobs).await;
        assert_eq!(results.len(), JEV_CONCURRENCY * 2 + 2, "全量完成");
        assert!(results.iter().all(Result::is_ok), "超额批次全成功");
        assert_eq!(JEV_CONCURRENCY, 8, "docs/05 §2：并发池 8");
    }

    #[tokio::test]
    async fn test_evaluate_many_keyless_reports_offline_per_slot() {
        let client = JevClient::with_options("http://localhost:9", DEFAULT_JEV_MODEL, None, None);
        let results = client.evaluate_many(vec![fanout_job("x")]).await;
        assert!(
            matches!(results[0], Err(JevError::Offline)),
            "无 key 每槽显式 Offline 不静默"
        );
    }
}
