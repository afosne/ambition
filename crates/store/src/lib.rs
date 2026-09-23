//! Ambition store：业务数据持久化（SQLite）
//! 存储分层（docs/02-core-store）：SQLite 只存业务数据；词库/内置内容 = data/ 资源文件。
//! 迁移：schema_version 单行表 + MIGRATIONS 清单（只增不改，应用到最大版本）。

// 模块分层（docs/01 §组织：lib.rs 薄壳，文件 ≤500 行，pre-commit 门禁强制）
mod arbitration;
mod attempts;
mod kv;
mod migration;
mod question_repo;
mod usage;

pub use arbitration::*;
pub use attempts::*;
pub use kv::*;
pub use migration::*;
pub use question_repo::*;
pub use usage::*;

use ambition_core::{
    Attempt, GradeResult, Provenance, Qid, Question, QuestionContent, QuestionKind, UsageSummary,
    UsageTotal,
};
use serde_json::Value;
use sqlx::sqlite::SqlitePool;
