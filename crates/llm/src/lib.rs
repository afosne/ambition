//! Ambition llm：外部 AI 适配层 —— OpenAI/Anthropic 协议 + 结构化输出路由（docs/05 §1）
//! 回包只经本层交给 06/07 消费；本 crate 不落业务库（账本走 `AiLedger` trait，SQLite 实现经 engine 装配）。
//!
//! 细分功能目录（docs/01 §1 组织规范）：
//! - `api/`      载荷与 trait 契约（GenRequest / LlmKind / LlmProvider / AiLedger + stub 契约测试）
//! - `provider/` 双协议供给方（OpenAI 兼容 / Anthropic + 共享请求执行环）
//! - `router/`   结构化路由与全局熔断（LlmRouter / Breaker）
//! - `ledger/`   进程内账本（MemLedger；SQLite 实现经 engine 装配）

mod api;
mod ledger;
mod provider;
mod router;

pub use api::*;
pub use ledger::MemLedger;
pub use provider::anthropic::Anthropic;
pub use provider::openai::OpenAiCompat;
pub use router::LlmRouter;
