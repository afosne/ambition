//! Ambition writing：模板作文域（M5-W4 落地，docs/10-writing §1；批注/加分项随 W5、语域随 W8）
//! 模板是脚手架，最终目标**脱模板**（填槽三阶段）：Full 句模板 → Half 段落功能提示 → Free 自由写。
//! 内置库 = `data/templates/*.json` include_str 内嵌（内容资源按 data/ 分层，docs/16 §3；运行时无文件读取）；
//! 个人模板在 SQLite `templates` 表（02-core-store §2 EssayRepo，W4b 落库）。

mod annotate;
mod template;

pub use annotate::*;
pub use template::*;
