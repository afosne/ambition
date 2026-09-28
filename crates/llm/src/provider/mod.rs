//! 供给方协议（细分域，docs/05 §1）：共享连接组件 + OpenAI 兼容 / Anthropic 双协议 + 请求执行环
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）

pub mod anthropic;
pub mod openai;
pub mod requests;

/// 双协议共享的连接组件（HTTP 客户端 + base/model/key）——
/// OpenAiCompat 与 Anthropic 状态同构、构造逻辑逐字相同，收口到此（2026-09-27 复用合并）
pub(crate) struct ProviderConn {
    pub(crate) http: reqwest::Client,
    pub(crate) base_url: String,
    pub(crate) api_key: Option<String>,
    pub(crate) model: String,
}

impl ProviderConn {
    /// 构造（base 尾部 `/` 归一化；api_key None = 离线形态，complete 报 Offline）
    pub(crate) fn new(
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_provider_conn_trims_base_url_and_keeps_key() {
        let conn = ProviderConn::new("http://x/v1/", "m1", Some("k".into()));
        assert_eq!(conn.base_url, "http://x/v1", "尾斜杠归一化");
        assert_eq!(conn.model, "m1");
        assert_eq!(conn.api_key.as_deref(), Some("k"));
        let keyless = ProviderConn::new("http://x", "m2", None);
        assert!(keyless.api_key.is_none(), "无 key = 离线形态");
    }
}
