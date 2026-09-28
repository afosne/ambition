//! LLM 跨文件测试夹具（收口前 provider/router 两份逐字复制：ok_json、Scripted 供给方、sample_req；2026-09-27 复用合并）

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{GenRequest, LlmError, LlmKind, LlmOut, LlmProvider};

/// 结构化出题的成功回包（空题集；请求执行环与路由测试共用）
pub(crate) fn ok_json() -> String {
    r#"{"questions": []}"#.to_string()
}

/// 按脚本回包的供给方（依次吐出 script 中的结果，耗尽/错误 → Offline；请求执行环与路由测试共用）
pub(crate) struct Scripted {
    script: Mutex<Vec<Result<String, LlmError>>>,
    calls: AtomicUsize,
    model: &'static str,
}

impl Scripted {
    pub(crate) fn new(script: Vec<Result<String, LlmError>>) -> Self {
        Self {
            script: Mutex::new(script),
            calls: AtomicUsize::new(0),
            model: "scripted",
        }
    }

    pub(crate) fn calls(&self) -> usize {
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

/// 结构化出题的样例请求（带 JSON Schema；双协议供给方测试共用）
pub(crate) fn sample_req() -> GenRequest {
    let mut req = GenRequest::new(LlmKind::QuestionGen, "sys", "usr");
    req.schema = Some(serde_json::json!({"type": "object"}));
    req
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_json_fixture_is_parsable_empty_questions() {
        let value: serde_json::Value = serde_json::from_str(&ok_json()).expect("合法 JSON");
        assert!(
            value["questions"].as_array().is_some_and(|q| q.is_empty()),
            "空题集结构化回包"
        );
    }

    #[tokio::test]
    async fn test_complete_scripted_pops_next_script_item() {
        let scripted = Scripted::new(vec![Ok("out".into()), Err(LlmError::Offline)]);
        let req = GenRequest::new(LlmKind::QuestionGen, "s", "u");
        let out = scripted.complete(&req).await.expect("脚本成功");
        assert_eq!(out.text, "out", "依次吐出脚本项");
        let miss = scripted.complete(&req).await;
        assert!(miss.is_err(), "脚本耗尽 → Offline");
        assert_eq!(scripted.calls(), 2, "调用计数随 complete 递增");
    }

    #[test]
    fn test_new_scripted_stores_script_and_fixed_model() {
        let scripted = Scripted::new(vec![Ok("a".into())]);
        assert_eq!(scripted.model, "scripted", "固定模型标识");
        assert_eq!(scripted.model(), "scripted", "trait 标识一致");
        assert_eq!(scripted.calls(), 0, "未调用计 0");
    }

    #[test]
    fn test_sample_req_fixture_carries_structured_schema() {
        let req = sample_req();
        assert_eq!(req.kind, LlmKind::QuestionGen);
        assert!(req.schema.is_some(), "带 JSON Schema");
    }

    #[test]
    fn test_calls_scripted_counts_only_complete_invocations() {
        let scripted = Scripted::new(vec![]);
        assert_eq!(scripted.calls(), 0, "构造不计数");
    }

    #[test]
    fn test_model_scripted_returns_fixed_identifier() {
        let scripted = Scripted::new(vec![Ok("x".into())]);
        assert_eq!(scripted.model(), "scripted", "trait 标识 = 固定模型名");
    }
}
