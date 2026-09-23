//! Ambition srs：间隔重复调度（M4 FSRS 接线 fsrs-rs；纯函数库，无 IO —— docs/08-adaptive）
//! 编排（取卡/落库/事件）在 engine（due_list/review_answer）；排程唯一实现 = 本 crate（00-overview §5）。

// 模块分层（docs/01 §组织：lib.rs 薄壳，文件 ≤500 行，pre-commit 门禁强制）
mod model;
mod schedule;

pub use model::*;
pub use schedule::*;

use ambition_core::{CardId, CardSource, ErrorTag, Verdict};
use fsrs::{FSRS, MemoryState};
