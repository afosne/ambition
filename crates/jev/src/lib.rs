//! Ambition jev：TypeSafe System One（Jev）HTTP 客户端（docs/05 §2）
//! M2 落地 `evaluate`（供 06 质检回路 fan-out）；M3 落地 `evaluate_many`（并发池 8、保序、单题失败不中断）
//! 与 `route_by_confidence` 置信度路由（0.75/0.35，非英语阈值由调用方传入）。
//! wire 契约（docs.typesafe.ai/api，2026-09-23 取证）：
//!   POST {base}/systemone  body {state, model, questions:{id:{type,instructions,criteria}}}
//!   resp {model, answers:{id:{type, noul|choice|score}}, usage:{input_tokens,output_tokens}}
//!   错误：401 key 无效 / 422 body 校验 / 429 限流 / 529 过载。

// 模块分层（docs/01 §组织：lib.rs 薄壳 + 细分功能目录 wire/client/route，文件 ≤500 行，pre-commit 门禁强制）
mod client;
mod route;
mod wire;

pub use client::error::*;
pub use client::*;
pub use route::*;
pub(crate) use wire::codec::*;
pub use wire::*;

use std::collections::BTreeMap;
use std::time::Duration;

use ambition_llm::{AiLedger, LlmError};
