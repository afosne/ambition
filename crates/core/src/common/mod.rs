//! 通用工具（细分域，docs/02 §1 / docs/08 §3）：标识与内容散列 / 文本规范化与假对检测 / 时钟 / 互斥锁
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）

pub mod analysis;
pub mod ids;
pub mod sync;
pub mod time;

pub use analysis::*;
pub use ids::*;
pub use sync::*;
pub use time::*;
