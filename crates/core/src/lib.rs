//! Ambition core：领域类型与纯函数（M1 骨架 + M2 出题/质检类型，设计见 docs/02-core-store）

// 模块分层（docs/01 §组织：lib.rs 薄壳，文件 ≤500 行，pre-commit 门禁强制）
mod analysis;
mod attempt;
mod event;
mod ids;
mod kind;
mod question;
mod taxonomy;
mod usage;

pub use analysis::*;
pub use attempt::*;
pub use event::*;
pub use ids::*;
pub use question::*;
pub use taxonomy::*;
pub use usage::*;

use sha2::{Digest, Sha256};
