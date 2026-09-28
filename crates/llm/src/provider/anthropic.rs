//! Anthropic Messages 协议（结构化输出强制 tool input_schema，docs/05 §1）

use crate::{GenRequest, LlmError, LlmOut, LlmProvider};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const EMIT_TOOL_NAME: &str = "emit_json";

/// Anthropic 供给方
pub struct Anthropic {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl Anthropic {
    /// 构造（base 如 https://api.anthropic.com/v1；api_key None = 离线）
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

    /// 组请求体（schema 存在 → 强制 tool_choice 调 emit_json）
    fn build_body(req: &GenRequest, model: &str) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "system": req.system,
            "messages": [{"role": "user", "content": req.user}],
        });
        if let Some(schema) = &req.schema {
            body["tools"] = serde_json::json!([{
                "name": EMIT_TOOL_NAME,
                "description": "Emit the structured response as tool input.",
                "input_schema": schema,
            }]);
            body["tool_choice"] = serde_json::json!({"type": "tool", "name": EMIT_TOOL_NAME});
        }
        body
    }

    /// 解析回包：tool_use.input（schema 模式）或 text block + usage
    fn parse_response(body: &serde_json::Value, model: &str) -> Result<LlmOut, LlmError> {
        let blocks = body
            .get("content")
            .and_then(serde_json::Value::as_array)
            .ok_or(LlmError::Http {
                status: 502,
                retryable: true,
            })?;
        let mut text: Option<String> = None;
        for block in blocks {
            match block.get("type").and_then(serde_json::Value::as_str) {
                Some("tool_use") => {
                    if let Some(input) = block.get("input") {
                        text = Some(input.to_string());
                        break;
                    }
                }
                Some("text") if text.is_none() => {
                    text = block
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                }
                _ => {}
            }
        }
        let text = text.ok_or(LlmError::Http {
            status: 502,
            retryable: true,
        })?;
        let usage = body.get("usage");
        Ok(LlmOut {
            text,
            model: model.to_string(),
            tokens_in: usage
                .and_then(|u| u.get("input_tokens"))
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            tokens_out: usage
                .and_then(|u| u.get("output_tokens"))
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
        })
    }

    /// 状态码 → 错误分类（429/529/5xx 可重试）
    fn classify_status(status: u16) -> LlmError {
        match status {
            429 | 529 | 500..=599 => LlmError::Http {
                status,
                retryable: true,
            },
            _ => LlmError::Http {
                status,
                retryable: false,
            },
        }
    }

    /// 发一次请求
    async fn send(&self, req: &GenRequest) -> Result<LlmOut, LlmError> {
        let api_key = self.api_key.as_deref().ok_or(LlmError::Offline)?;
        let url = format!("{}/messages", self.base_url);
        let resp = self
            .http
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&Self::build_body(req, &self.model))
            .send()
            .await
            .map_err(|_| LlmError::Http {
                status: 0,
                retryable: true,
            })?;
        let status = resp.status().as_u16();
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
impl LlmProvider for Anthropic {
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
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_new_trims_base_url_and_defaults_keyless() {
        let p = Anthropic::new("https://api.anthropic.com/v1/", "claude", None);
        assert_eq!(p.base_url, "https://api.anthropic.com/v1");
        assert!(p.api_key.is_none());
        assert_eq!(p.model, "claude");
    }

    #[test]
    fn test_build_body_forces_tool_input_schema() {
        let req = sample_req();
        let body = Anthropic::build_body(&req, "claude");
        assert_eq!(body["tools"][0]["name"], "emit_json");
        assert_eq!(
            body["tool_choice"]["type"], "tool",
            "schema 模式强制 tool 调用"
        );
        assert_eq!(body["system"], "sys");
        let plain = Anthropic::build_body(&GenRequest::new(LlmKind::Explain, "s", "u"), "claude");
        assert!(
            plain.get("tools").is_none(),
            "无 schema 不带 tools（纯文本）"
        );
    }

    #[test]
    fn test_parse_response_prefers_tool_use_input() {
        let body = serde_json::json!({
            "content": [
                {"type": "text", "text": "ignored"},
                {"type": "tool_use", "input": {"questions": []}}
            ],
            "usage": {"input_tokens": 12, "output_tokens": 3}
        });
        let out = Anthropic::parse_response(&body, "claude").expect("解析");
        assert_eq!(out.text, "{\"questions\":[]}");
        assert_eq!(out.tokens_in, 12);
        assert_eq!(out.tokens_out, 3);

        let text_body = serde_json::json!({
            "content": [{"type": "text", "text": "plain"}],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });
        let out2 = Anthropic::parse_response(&text_body, "claude").expect("text 路径");
        assert_eq!(out2.text, "plain");

        let broken = serde_json::json!({"content": []});
        assert!(
            Anthropic::parse_response(&broken, "claude").is_err(),
            "空 content 报错"
        );
    }

    #[test]
    fn test_classify_status_marks_529_retryable() {
        assert!(matches!(
            Anthropic::classify_status(529),
            LlmError::Http {
                status: 529,
                retryable: true
            }
        ));
        assert!(matches!(
            Anthropic::classify_status(400),
            LlmError::Http {
                status: 400,
                retryable: false
            }
        ));
    }

    #[test]
    fn test_model_reports_configured_identifier() {
        let provider = Anthropic::new("http://x/v1", "claude-opus", None);
        assert_eq!(provider.model(), "claude-opus");
    }

    #[tokio::test]
    async fn test_send_keyless_short_circuits_offline() {
        let provider = Anthropic::new("http://127.0.0.1:1/v1", "claude", None);
        let err = provider
            .send(&GenRequest::new(LlmKind::Explain, "s", "u"))
            .await
            .expect_err("无 key");
        assert!(matches!(err, LlmError::Offline));
    }

    #[tokio::test]
    async fn test_complete_wire_contract_sends_version_and_tool_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(header("x-api-key", "k"))
            .and(body_string_contains("emit_json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{"type": "tool_use", "input": {"questions": [1]}}],
                "usage": {"input_tokens": 8, "output_tokens": 4}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider = Anthropic::new(format!("{}/v1", server.uri()), "claude", Some("k".into()));
        let out = provider.complete(&sample_req()).await.expect("协议成功");
        assert_eq!(out.text, "{\"questions\":[1]}");
        assert_eq!(provider.model(), "claude");
    }

    #[tokio::test]
    async fn test_complete_keyless_reports_offline() {
        let provider = Anthropic::new("http://127.0.0.1:1/v1", "claude", None);
        let err = provider.complete(&sample_req()).await.expect_err("无 key");
        assert!(matches!(err, LlmError::Offline), "无 key → Offline: {err}");
    }
}
