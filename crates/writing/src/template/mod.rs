//! 模板作文域（细分域，docs/10 §1，M5-W4）：类型 + 三阶段脚手架 + 套槽渲染 + 内置库
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）

pub mod library;
pub mod model;
pub mod render;
pub mod scaffold;

pub use library::{builtin, builtin_templates};
pub use model::*;
pub use render::render;
pub use scaffold::scaffold;
