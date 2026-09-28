//! Ambition core：领域类型与纯函数（M1 骨架 + M2 出题/质检类型，设计见 docs/02-core-store）

// 模块分层（docs/01 §组织：lib.rs 薄壳 + 细分功能目录 model/common，文件 ≤500 行，pre-commit 门禁强制）
mod common;
mod model;

pub use common::*;
pub use model::*;

use sha2::{Digest, Sha256};
