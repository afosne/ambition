//! 测试通用件（细分域，仅测试构建；docs/01 §1 组织规范）：跨文件共享夹具
//!
//! 收口前 `memory_pool` 在 7 个文件、`sample_*` 在 3 个文件逐字复制（2026-09-27 复用合并）。
//! lib.rs 以 `#[cfg(test)] mod common;` 声明——非测试构建不参与编译。

pub(crate) mod fixtures;

pub(crate) use fixtures::*;
