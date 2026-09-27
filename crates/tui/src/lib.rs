//! Ambition TUI：壳层状态机（纯 update + 渲染，设计见 docs/03-engine-ui §3）
//! M2：填空输入行 · 判分错误独立展示（≠ Pending，P-8/改进 #6）· 提交计时（假对检测输入）
//! M3：待裁定 y/n 裁决回路 · 判分浮层错因标签（结构化讲解①）· 1-4 自评把握（改进 #10）
//! 2026-09-27 键位统一：keys.rs（KeyCtx 唯一路由 + hint 同源）· Space 多选 · Esc 退出编辑保留输入

// 模块分层（docs/01 §组织：lib.rs 薄壳，文件 ≤500 行，pre-commit 门禁强制）
mod keys;
mod render;
mod review_render;
mod review_state;
mod state;
mod update;
#[cfg(test)]
mod update_tests;

pub use keys::*;
pub use render::*;
pub use review_render::*;
pub use review_state::*;
pub use state::*;
pub use update::*;

use std::time::Instant;

use ambition_core::{Answer, GradeResult, Question, QuestionKind, Timing};
use ambition_engine::CardAction;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::{Style, Stylize};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
