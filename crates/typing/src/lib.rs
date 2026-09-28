//! Ambition typing：打字通「键胜」纯逻辑（设计见 docs/04-typing）
//!
//! 细分功能目录（docs/01 §1 组织规范）：
//! - `model/`   数据模型（KeyStroke / TypingStats）
//! - `metrics/` 指标计算（stats / heat / bottleneck_keys / baseline_wpm）

mod metrics;
mod model;

pub use metrics::*;
pub use model::*;
