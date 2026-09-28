//! wire 线格式（细分域，docs/05 §2）：Jev 状态/问题/答案载荷与编解码
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）

pub mod codec;
pub mod types;

pub use types::*;
