//! OpenAI 兼容协议（chat/completions；含 DeepSeek/Qwen/Moonshot/Ollama 同款路径）
//! response_format=json_schema 约束结构化输出，400/422 时降级 json_object + schema 内嵌 system。

use crate::{GenRequest, JsonSchema, LlmError, LlmOut, LlmProvider};

/// OpenAI 兼容供给方
pub struct OpenAiCompat {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiCompat {
    /// 构造（base 如 http://127.0.0.1:PORT/v1；api_key None = 离线，complete 报 Offline）
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            model: model.into(),
        }
    }

    /// 组请求体（json_schema 模式；schema=None 时纯文本）
    fn build_body(req: &GenRequest, model: &str, json_schema_mode: bool) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": model,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
            "messages": [
                {"role": "system", "content": req.system},
                {"role": "user", "content": req.user},
            ],
        });
        if json_schema_mode && let Some(schema) = &req.schema {
            body["response_format"] = serde_json::json!({
                "type": "json_schema",
                "json_schema": {"name": "response", "strict": true, "schema": schema},
            });
        }
        body
    }

    /// 降级模式请求体：json_object + schema 内嵌 system
    fn build_fallback_body(req: &GenRequest, model: &str) -> serde_json::Value {
        let schema_note = req
            .schema
            .as_ref()
            .map_or_else(String::new, |schema: &JsonSchema| {
                format!("\n\nRespond ONLY with JSON matching this schema: {schema}")
            });
        let mut degraded = req.clone();
        degraded.system = format!("{}{schema_note}", req.system);
        degraded.schema = None;
        let mut body = Self::build_body(&degraded, model, false);
        body["response_format"] = serde_json::json!({"type": "json_object"});
        body
    }

    /// 状态码 → 错误分类（5xx/429 可重试；401/403/400 不可重试）
    fn classify_status(status: u16) -> LlmError {
        match status {
            429 | 500..=599 => LlmError::Http {
                status,
                retryable: true,
            },
            _ => LlmError::Http {
                status,
                retryable: false,
            },
        }
    }

    /// 解析回包：choices[0].message.content + usage
    fn parse_response(body: &serde_json::Value, model: &str) -> Result<LlmOut, LlmError> {
        let text = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or(LlmError::Http {
                status: 502,
                retryable: true,
            })?
            .to_string();
        let usage = body.get("usage");
        Ok(LlmOut {
            text,
            model: model.to_string(),
            tokens_in: usage
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            tokens_out: usage
                .and_then(|u| u.get("completion_tokens"))
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
        })
    }

    /// 发一次请求（schema 模式先试 json_schema，400/422 自动降级 json_object）
    async fn send(&self, req: &GenRequest) -> Result<LlmOut, LlmError> {
        let api_key = self.api_key.as_deref().ok_or(LlmError::Offline)?;
        let url = format!("{}/chat/completions", self.base_url);
        let json_schema_mode = req.schema.is_some();

        let resp = self
            .http
            .post(&url)
            .bearer_auth(api_key)
            .json(&Self::build_body(req, &self.model, json_schema_mode))
            .send()
            .await
            .map_err(|_| LlmError::Http {
                status: 0,
                retryable: true,
            })?;
        let status = resp.status().as_u16();

        // 结构化模式被拒（旧供应商不认 json_schema）→ 降级 json_object + 内嵌 schema 重发
        if json_schema_mode && (status == 400 || status == 422) {
            let fallback = self
                .http
                .post(&url)
                .bearer_auth(api_key)
                .json(&Self::build_fallback_body(req, &self.model))
                .send()
                .await
                .map_err(|_| LlmError::Http {
                    status: 0,
                    retryable: true,
                })?;
            let fb_status = fallback.status().as_u16();
            if !(200..300).contains(&fb_status) {
                return Err(Self::classify_status(fb_status));
            }
            let body = fallback
                .json::<serde_json::Value>()
                .await
                .map_err(|_| LlmError::Http {
                    status: 502,
                    retryable: true,
                })?;
            return Self::parse_response(&body, &self.model);
        }

        // 非成功状态先分类（401 等错误体可能非 JSON，不能先 parse）
        if !(200..300).contains(&status) {
            return Err(Self::classify_status(status));
        }

        let body = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|_| LlmError::Http {
                status: 502,
                retryable: true,
            })?;
        Self::parse_response(&body, &self.model)
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompat {
    async fn complete(&self, req: &GenRequest) -> Result<LlmOut, LlmError> {
        self.send(req).await
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LlmKind;
    use crate::common::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ok_body() -> serde_json::Value {
        serde_json::json!({
            "choices": [{"message": {"content": "{\"questions\": []}"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        })
    }

    #[test]
    fn test_new_trims_base_url_and_defaults_keyless() {
        let p = OpenAiCompat::new("http://x/v1/", "m1", None);
        assert_eq!(p.base_url, "http://x/v1", "尾斜杠修剪");
        assert!(p.api_key.is_none());
        assert_eq!(p.model, "m1");
    }

    #[test]
    fn test_build_body_embeds_json_schema_response_format() {
        let req = sample_req();
        let body = OpenAiCompat::build_body(&req, "m1", true);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["messages"][0]["role"], "system");
        let plain = OpenAiCompat::build_body(&req, "m1", false);
        assert!(
            plain.get("response_format").is_none(),
            "非 schema 模式不带 response_format"
        );
    }

    #[test]
    fn test_build_fallback_body_uses_json_object_and_inlines_schema() {
        let req = sample_req();
        let body = OpenAiCompat::build_fallback_body(&req, "m1");
        assert_eq!(body["response_format"]["type"], "json_object");
        let system = body["messages"][0]["content"].as_str().expect("system");
        assert!(
            system.contains("Respond ONLY with JSON"),
            "schema 内嵌 system: {system}"
        );
        assert!(
            system.contains("\"type\":\"object\""),
            "schema 全文在场（serde 紧凑格式）: {system}"
        );
    }

    #[test]
    fn test_classify_status_marks_retryable() {
        assert!(matches!(
            OpenAiCompat::classify_status(429),
            LlmError::Http {
                status: 429,
                retryable: true
            }
        ));
        assert!(matches!(
            OpenAiCompat::classify_status(503),
            LlmError::Http {
                status: 503,
                retryable: true
            }
        ));
        assert!(matches!(
            OpenAiCompat::classify_status(401),
            LlmError::Http {
                status: 401,
                retryable: false
            }
        ));
    }

    #[test]
    fn test_parse_response_extracts_content_and_usage() {
        let out = OpenAiCompat::parse_response(&ok_body(), "m1").expect("解析");
        assert_eq!(out.text, "{\"questions\": []}");
        assert_eq!(out.tokens_in, 10);
        assert_eq!(out.tokens_out, 5);
        let missing = serde_json::json!({"choices": []});
        assert!(
            OpenAiCompat::parse_response(&missing, "m1").is_err(),
            "缺字段报错"
        );
    }

    #[test]
    fn test_model_reports_configured_identifier() {
        let provider = OpenAiCompat::new("http://x/v1", "gpt-x", None);
        assert_eq!(provider.model(), "gpt-x");
    }

    #[tokio::test]
    async fn test_send_keyless_short_circuits_offline() {
        let provider = OpenAiCompat::new("http://127.0.0.1:1/v1", "m1", None);
        let err = provider
            .send(&GenRequest::new(LlmKind::Explain, "s", "u"))
            .await
            .expect_err("无 key");
        assert!(matches!(err, LlmError::Offline));
    }

    #[tokio::test]
    async fn test_complete_wire_contract_success_and_keyless_offline() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
            .expect(1)
            .mount(&server)
            .await;

        let provider = OpenAiCompat::new(format!("{}/v1", server.uri()), "m1", Some("k".into()));
        let out = provider
            .complete(&sample_req())
            .await
            .expect("协议成功路径");
        assert_eq!(out.model, "m1");

        let keyless = OpenAiCompat::new(format!("{}/v1", server.uri()), "m1", None);
        let err = keyless
            .complete(&sample_req())
            .await
            .expect_err("无 key 离线");
        assert!(matches!(err, LlmError::Offline), "无 key → Offline: {err}");
    }

    #[tokio::test]
    async fn test_complete_falls_back_to_json_object_on_400() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_json(
                serde_json::json!({"error": {"message": "response_format unsupported"}}),
            ))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
            .mount(&server)
            .await;

        let provider = OpenAiCompat::new(format!("{}/v1", server.uri()), "m1", Some("k".into()));
        let out = provider
            .complete(&sample_req())
            .await
            .expect("降级路径成功");
        assert_eq!(out.text, "{\"questions\": []}");
    }

    #[tokio::test]
    async fn test_complete_maps_unauthorized_to_non_retryable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let provider = OpenAiCompat::new(format!("{}/v1", server.uri()), "m1", Some("k".into()));
        let err = provider.complete(&sample_req()).await.expect_err("401");
        assert!(
            matches!(
                err,
                LlmError::Http {
                    status: 401,
                    retryable: false
                }
            ),
            "401 不可重试: {err}"
        );
    }
}
