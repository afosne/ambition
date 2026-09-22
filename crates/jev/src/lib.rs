//! Ambition jev：TypeSafe System One（Jev）HTTP 客户端（docs/05 §2）
//! M2 落地 `evaluate`（供 06 质检回路 fan-out）；M3 落地 `evaluate_many`（并发池 8、保序、单题失败不中断）
//! 与 `route_by_confidence` 置信度路由（0.75/0.35，非英语阈值由调用方传入）。
//! wire 契约（docs.typesafe.ai/api，2026-09-23 取证）：
//!   POST {base}/systemone  body {state, model, questions:{id:{type,instructions,criteria}}}
//!   resp {model, answers:{id:{type, noul|choice|score}}, usage:{input_tokens,output_tokens}}
//!   错误：401 key 无效 / 422 body 校验 / 429 限流 / 529 过载。

use std::collections::BTreeMap;
use std::time::Duration;

use ambition_llm::{AiLedger, LlmError};

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

/// fan-out 作业：一次作答一单（state + 判断集，M3）
#[derive(Debug, Clone)]
pub struct JevJob {
    pub state: JevState,
    pub questions: JevQuestions,
}

/// 路由决策（docs/05 §2：阈值两端采信、灰区升级复核，M3）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// 高置信：采信判断（Noul 采信「是」）
    AutoAccept,
    /// 低置信：不采信该判断（Noul 采信「否」——确定性否定）
    AutoReject,
    /// 灰区：升级 LLM 粗判（只出判断不打分）
    LlmReview,
    /// LLM 复核不可用时的人工裁决（由 07 降级填入，不直接由阈值产生）
    HumanReview,
}

/// 阈值路由（纯 margin 版；`route_by_confidence` 的底座，grader 按裸 margin 复用）
pub fn route_from_margin(margin: f32, accept: f32, reject: f32) -> Route {
    if margin >= accept {
        Route::AutoAccept
    } else if margin <= reject {
        Route::AutoReject
    } else {
        Route::LlmReview
    }
}

/// 置信度路由（docs/05 §2：margin ≥ accept → AutoAccept；margin ≤ reject → AutoReject；灰区 → LlmReview）
/// Noul 的 margin = yes_prob（有向）；Choice/Score 的 margin = confidence。
pub fn route_by_confidence(ans: &JevAnswer, accept: f32, reject: f32) -> Route {
    let margin = match ans {
        JevAnswer::Noul { yes_prob } => *yes_prob as f32,
        JevAnswer::Choice { confidence, .. } => *confidence as f32,
        JevAnswer::Score { confidence, .. } => *confidence as f32,
    };
    route_from_margin(margin, accept, reject)
}

/// Jev 状态（只放 l2 文本；母语/中文永不进 state，docs/05 §3.5）
#[derive(Debug, Clone)]
pub struct JevState(pub serde_json::Value);

/// 三原语之一：选项判断
#[derive(Debug, Clone)]
pub struct ChoiceQ {
    pub instructions: String,
    pub criteria: BTreeMap<String, String>,
}

/// 三原语之一：按档位打分（criteria = 有序 levels，2–10 档）
#[derive(Debug, Clone)]
pub struct ScoreQ {
    pub instructions: String,
    pub levels: Vec<String>,
}

/// 三原语之一：是/否概率
#[derive(Debug, Clone)]
pub struct NoulQ {
    pub instructions: String,
}

/// 质检问题集（id → 原语；顺序确定便于测试断言）
pub type JevQuestions = BTreeMap<String, JevQuestion>;

/// 单个问题（一个判断）
#[derive(Debug, Clone)]
pub enum JevQuestion {
    Choice(ChoiceQ),
    Score(ScoreQ),
    Noul(NoulQ),
}

/// 质检回答集（与 questions 同 id）
pub type JevAnswers = BTreeMap<String, JevAnswer>;

/// 单个回答
#[derive(Debug, Clone, PartialEq)]
pub enum JevAnswer {
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Score {
        score: f64,
        confidence: f64,
    },
    Noul {
        /// 是的概率 ∈ [0,1]（wire 字段名 noul）
        yes_prob: f64,
    },
}

impl JevAnswer {
    /// 取 noul 概率（非 noul → None，调用方按缺答处理）
    pub fn noul_prob(&self) -> Option<f64> {
        match self {
            Self::Noul { yes_prob } => Some(*yes_prob),
            _ => None,
        }
    }

    /// 取分数（非 score → None）
    pub fn score_value(&self) -> Option<f64> {
        match self {
            Self::Score { score, .. } => Some(*score),
            _ => None,
        }
    }
}

/// Jev 错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum JevError {
    #[error("jev http {status} (retryable={retryable})")]
    Http { status: u16, retryable: bool },
    #[error("jev invalid response: {0}")]
    Invalid(String),
    #[error("jev offline: no api key configured")]
    Offline,
    /// 记账失败不阻断判题（账本是旁路）
    #[error("jev ledger failed: {0}")]
    Ledger(String),
}

impl From<LlmError> for JevError {
    fn from(err: LlmError) -> Self {
        match err {
            LlmError::Http { status, retryable } => Self::Http { status, retryable },
            LlmError::Offline => Self::Offline,
            other => Self::Invalid(other.to_string()),
        }
    }
}

/// 状态码 → 错误分类（429/529/5xx 可重试；401/422 不可重试）
pub fn classify_jev_status(status: u16) -> JevError {
    match status {
        429 | 529 | 500..=599 => JevError::Http {
            status,
            retryable: true,
        },
        _ => JevError::Http {
            status,
            retryable: false,
        },
    }
}

/// 序列化问题 → wire 形态（{type, instructions, criteria}）
fn encode_question(q: &JevQuestion) -> serde_json::Value {
    match q {
        JevQuestion::Noul(NoulQ { instructions }) => {
            serde_json::json!({"type": "noul", "instructions": instructions})
        }
        JevQuestion::Choice(ChoiceQ {
            instructions,
            criteria,
        }) => {
            serde_json::json!({"type": "choice", "instructions": instructions, "criteria": criteria})
        }
        JevQuestion::Score(ScoreQ {
            instructions,
            levels,
        }) => {
            serde_json::json!({"type": "score", "instructions": instructions, "criteria": levels})
        }
    }
}

/// 组请求体（state 只含 l2；model 默认 jev-latest）
fn encode_request(state: &JevState, model: &str, questions: &JevQuestions) -> serde_json::Value {
    let encoded: serde_json::Map<String, serde_json::Value> = questions
        .iter()
        .map(|(id, q)| (id.clone(), encode_question(q)))
        .collect();
    serde_json::json!({"state": state.0, "model": model, "questions": encoded})
}

/// 解析回答（type 字段分派；缺 type = 响应不符契约）
fn decode_answer(value: &serde_json::Value) -> Option<JevAnswer> {
    match value.get("type")?.as_str()? {
        "noul" => Some(JevAnswer::Noul {
            yes_prob: value.get("noul")?.as_f64()?,
        }),
        "choice" => {
            let choice = value.get("choice")?.as_str()?.to_string();
            let probabilities = value
                .get("probabilities")?
                .as_object()?
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                .collect();
            Some(JevAnswer::Choice {
                choice,
                probabilities,
                confidence: value.get("confidence")?.as_f64()?,
            })
        }
        "score" => Some(JevAnswer::Score {
            score: value.get("score")?.as_f64()?,
            confidence: value.get("confidence")?.as_f64()?,
        }),
        _ => None,
    }
}

/// 解析响应体（answers 缺失/单条损坏 → Err）
fn decode_response(body: &serde_json::Value) -> Result<JevAnswers, JevError> {
    let answers = body
        .get("answers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| JevError::Invalid("missing answers".into()))?;
    let mut out = JevAnswers::new();
    for (id, value) in answers {
        let answer =
            decode_answer(value).ok_or_else(|| JevError::Invalid(format!("bad answer: {id}")))?;
        out.insert(id.clone(), answer);
    }
    Ok(out)
}

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

    #[test]
    fn test_classify_jev_status_marks_rate_limit_retryable() {
        assert!(matches!(
            classify_jev_status(429),
            JevError::Http {
                status: 429,
                retryable: true
            }
        ));
        assert!(matches!(
            classify_jev_status(529),
            JevError::Http {
                status: 529,
                retryable: true
            }
        ));
        assert!(matches!(
            classify_jev_status(422),
            JevError::Http {
                status: 422,
                retryable: false
            }
        ));
        assert!(matches!(
            classify_jev_status(401),
            JevError::Http {
                status: 401,
                retryable: false
            }
        ));
    }

    #[test]
    fn test_encode_question_covers_three_primitives() {
        let qs = sample_questions();
        let body = encode_request(&JevState(serde_json::json!("stem")), "jev-latest", &qs);
        assert_eq!(body["model"], "jev-latest");
        assert_eq!(body["questions"]["unique_answer"]["type"], "noul");
        assert_eq!(body["questions"]["natural"]["type"], "score");
        assert_eq!(
            body["questions"]["natural"]["criteria"][2], "natural",
            "score criteria = 有序 levels 数组"
        );
        assert_eq!(body["questions"]["error_cause"]["type"], "choice");
        assert_eq!(
            body["questions"]["error_cause"]["criteria"]["word_sense"],
            "wrong sense"
        );
        assert_eq!(body["state"], "stem", "state 原样透传");
    }

    #[test]
    fn test_encode_request_keeps_state_cjk_free() {
        // docs/05 §3.5：state 只放 l2 —— 此断言保证调用方拼的 state 未混入母语提示
        let state = JevState(serde_json::json!({
            "sentence": "She is ___ student.",
            "options": ["a", "an", "the"],
            "answer": "an"
        }));
        let body = encode_request(&state, "jev-latest", &sample_questions());
        let text = body.to_string();
        assert!(
            !text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "state 无 CJK 母语内容: {text}"
        );
    }

    #[test]
    fn test_decode_response_parses_all_primitives_and_rejects_garbage() {
        let answers = decode_response(&ok_body()).expect("解析");
        assert_eq!(
            answers.get("unique_answer").and_then(JevAnswer::noul_prob),
            Some(0.95),
            "noul → yes_prob"
        );
        assert_eq!(
            answers.get("natural").and_then(JevAnswer::score_value),
            Some(1.9)
        );
        match answers.get("error_cause") {
            Some(JevAnswer::Choice {
                choice, confidence, ..
            }) => {
                assert_eq!(choice, "word_sense");
                assert!((*confidence - 0.88).abs() < 1e-9);
            }
            other => panic!("choice 解析错误: {other:?}"),
        }

        let missing = serde_json::json!({"model": "x"});
        assert!(decode_response(&missing).is_err(), "缺 answers 报错");
        let broken = serde_json::json!({"answers": {"a": {"type": "mystery"}}});
        assert!(decode_response(&broken).is_err(), "未知 type 报错");
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
    fn test_noul_prob_and_score_value_helpers() {
        let noul = JevAnswer::Noul { yes_prob: 0.7 };
        assert_eq!(noul.noul_prob(), Some(0.7));
        assert_eq!(noul.score_value(), None);
        let score = JevAnswer::Score {
            score: 1.5,
            confidence: 0.9,
        };
        assert_eq!(score.score_value(), Some(1.5));
        assert_eq!(score.noul_prob(), None);
    }

    #[test]
    fn test_score_value_reads_only_score_answers() {
        let choice = JevAnswer::Choice {
            choice: "a".into(),
            probabilities: BTreeMap::new(),
            confidence: 0.5,
        };
        assert_eq!(choice.score_value(), None);
        assert_eq!(
            JevAnswer::Score {
                score: 2.0,
                confidence: 1.0
            }
            .score_value(),
            Some(2.0)
        );
    }

    #[test]
    fn test_decode_answer_dispatches_by_type_field() {
        let noul = serde_json::json!({"type": "noul", "noul": 0.4});
        assert_eq!(decode_answer(&noul).and_then(|a| a.noul_prob()), Some(0.4));
        assert!(
            decode_answer(&serde_json::json!({"type": "unknown"})).is_none(),
            "未知 type → None"
        );
        assert!(
            decode_answer(&serde_json::json!({"type": "noul"})).is_none(),
            "缺字段 → None"
        );
    }

    #[test]
    fn test_from_llm_error_converts_domains() {
        let from_http = JevError::from(LlmError::Http {
            status: 500,
            retryable: true,
        });
        assert!(matches!(
            from_http,
            JevError::Http {
                status: 500,
                retryable: true
            }
        ));
        assert!(matches!(
            JevError::from(LlmError::Offline),
            JevError::Offline
        ));
        let other = JevError::from(LlmError::InvalidOutput { attempts: 3 });
        assert!(matches!(other, JevError::Invalid(_)));
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

    fn fanout_job(marker: &str) -> JevJob {
        JevJob {
            state: JevState(serde_json::json!({ "stem": marker })),
            questions: sample_questions(),
        }
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

    #[test]
    fn test_route_from_margin_orders_three_bands() {
        assert_eq!(route_from_margin(0.75, 0.75, 0.35), Route::AutoAccept);
        assert_eq!(route_from_margin(0.74, 0.75, 0.35), Route::LlmReview);
        assert_eq!(route_from_margin(0.35, 0.75, 0.35), Route::AutoReject);
        assert_eq!(route_from_margin(0.36, 0.75, 0.35), Route::LlmReview);
        assert_eq!(route_from_margin(0.0, 0.75, 0.35), Route::AutoReject);
        assert_eq!(route_from_margin(1.0, 0.75, 0.35), Route::AutoAccept);
        // 退化阈值（reject ≥ accept）仍不 panic：先判 accept
        assert_eq!(route_from_margin(0.5, 0.5, 0.5), Route::AutoAccept);
    }

    #[test]
    fn test_route_by_confidence_boundary_table() {
        let noul = |p: f64| JevAnswer::Noul { yes_prob: p };
        let choice = |c: f64| JevAnswer::Choice {
            choice: "x".into(),
            probabilities: BTreeMap::new(),
            confidence: c,
        };
        let score = |c: f64| JevAnswer::Score {
            score: 1.0,
            confidence: c,
        };
        let (accept, reject) = (0.75, 0.35); // 英语默认阈值
        // 边界（=阈值）表驱动：两端闭区间采信
        let table: Vec<(JevAnswer, Route)> = vec![
            (noul(0.75), Route::AutoAccept),
            (noul(0.749), Route::LlmReview),
            (noul(0.5), Route::LlmReview),
            (noul(0.351), Route::LlmReview),
            (noul(0.35), Route::AutoReject),
            (choice(0.9), Route::AutoAccept),
            (choice(0.75), Route::AutoAccept),
            (choice(0.5), Route::LlmReview),
            (choice(0.35), Route::AutoReject),
            (score(0.8), Route::AutoAccept),
            (score(0.1), Route::AutoReject),
            (score(0.4), Route::LlmReview),
        ];
        for (ans, expected) in &table {
            assert_eq!(
                route_by_confidence(ans, accept, reject),
                *expected,
                "路由不符: {ans:?}"
            );
        }
    }

    #[test]
    fn test_route_by_confidence_switches_to_non_english_thresholds() {
        // docs/05 §3：其它语种 0.85/0.20（更保守：更多灰区进复核）
        let (accept, reject) = (0.85, 0.20);
        let p80 = JevAnswer::Noul { yes_prob: 0.80 };
        assert_eq!(
            route_by_confidence(&p80, accept, reject),
            Route::LlmReview,
            "en 下 0.80 自动接受，其它语种进灰区"
        );
        assert_eq!(
            route_by_confidence(&p80, 0.75, 0.35),
            Route::AutoAccept,
            "英语阈值同输入不同结论"
        );
        assert_eq!(
            route_by_confidence(&JevAnswer::Noul { yes_prob: 0.85 }, accept, reject),
            Route::AutoAccept,
            "=0.85 边界采信"
        );
        assert_eq!(
            route_by_confidence(&JevAnswer::Noul { yes_prob: 0.20 }, accept, reject),
            Route::AutoReject,
            "=0.20 边界拒绝"
        );
        assert_eq!(
            route_by_confidence(&JevAnswer::Noul { yes_prob: 0.21 }, accept, reject),
            Route::LlmReview,
            "0.21 进灰区"
        );
    }
}
