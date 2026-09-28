//! Ambition gamify：游戏层底座（事件订阅旁路，可整体关闭）
//! 设计见 docs/13-gamify-growth —— 只做反馈与乐趣，不影响排程/评分。
//! M2：GameProjector 实现 core::EventBus（engine 只 publish 不直调，改进 #3），效果由 engine 落 game_state（改进 #1）。
//!
//! 细分功能目录（docs/01 §1 组织规范）：
//! - `model/`   数据模型（GameEffect / GameState）
//! - `projector/` 投影器（EventBus 订阅 + 状态水合/快照/效果结算）
//! - `rules/`   游戏规则（事件映射 / 等级曲线 / 一局结算）

mod model;
mod projector;
mod rules;

pub use model::*;
pub use projector::GameProjector;
pub use rules::*;
