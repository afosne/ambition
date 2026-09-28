//! 供给方协议（细分域，docs/05 §1）：共享请求执行环 + OpenAI 兼容 / Anthropic 双协议
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）

pub mod anthropic;
pub mod openai;
pub mod requests;
