//! Ambition TUI：壳层状态机（纯 update + 渲染，设计见 docs/03-engine-ui §3）
//! M2：填空输入行 · 判分错误独立展示（≠ Pending，P-8/改进 #6）· 提交计时（假对检测输入）
//! M3：待裁定 y/n 裁决回路 · 判分浮层错因标签（结构化讲解①）· 1-4 自评把握（改进 #10）

// 模块分层（docs/01 §组织：lib.rs 薄壳，文件 ≤500 行，pre-commit 门禁强制）
mod input;
mod render;
mod state;
mod update;

pub use input::*;
pub use render::*;
pub use state::*;
pub use update::*;

use std::time::Instant;

use ambition_core::{Answer, GradeResult, Question, QuestionKind, Timing};
use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::{Style, Stylize};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
