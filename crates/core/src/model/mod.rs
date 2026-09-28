//! 领域模型（细分域，docs/02-core-store §2-§5）：题目 / 作答判分载荷 / 枚举词表 / 台账 / 事件
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）

pub mod attempt;
pub mod event;
pub mod paper;
pub mod question;
pub mod taxonomy;
pub mod usage;
// kind 仅承载 QuestionKind 的 impl 辅助（无独立导出项），保持域内私有
mod kind;

pub use attempt::*;
pub use event::*;
pub use paper::*;
pub use question::*;
pub use taxonomy::*;
pub use usage::*;
