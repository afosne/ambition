//! Ambition translate：翻译工作台域（M5-W7，docs/11）——类型 / 提示词与 schema / 对齐差异纯件。
//! AI 调用与落库在 engine（docs/03 translate_* 门面）；评分复用 07-grading
//! （l2=en 唯一评分路径；其它语种降级 grade=None、随多语 LLM 粗判——docs/11 §2）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

mod diff;
mod model;
mod prompt;

pub use diff::diff;
pub use model::*;
pub use prompt::{
    TranslateWire, align_schema, render_align_prompt, render_translate_prompt, translate_schema,
};
