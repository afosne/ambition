//! Ambition grader：三级判题流水线（docs/07-grading）
//! M1 规则层在 `rule`（纯规则判定）；M3 落地 `pipeline::Grader`（规则 → Jev → LLM 复核/待裁定桶）
//! 与 `corpus`（assets/jev/<l2>.json 判分语料 + 错因归类口）。
//!
//! 细分功能目录（docs/01 §1 组织规范）：
//! - `rule/`     一二级规则层（纯规则判定 + 判定环子组件 ring/ring_tests）
//! - `pipeline/` 三级流水线（Grader 编排 + Jev 升级 / LLM 复核子组件）
//! - `common/`   判分通用（共用辅助 / 判词载荷 / 语言阈值带）
//! - `corpus/`   判分语料（按 l2 分版、缺版回退 en）

// 模块分层（docs/01 §组织：lib.rs 薄壳，文件 ≤500 行，pre-commit 门禁强制）
mod common;
mod corpus;
mod pipeline;
mod rule;

pub use common::judges::*;
pub use common::thresholds::*;
pub(crate) use common::*;
pub use corpus::{
    ERROR_TAGS_VERSION, corpus_version, error_cause_question, error_cause_questions,
    error_criteria, tag_error_cause,
};
pub use pipeline::*;
pub use rule::*;

use ambition_core::{
    Answer, ChoiceBody, ErrorTag, FillBody, GradeResult, GradedBy, ItemResult, Question,
    QuestionKind, Verdict, normalize_text,
};
use ambition_jev::Route;
use ambition_jev::{
    JevClient, JevError, JevQuestion, JevQuestions, JevState, NoulQ, route_from_margin,
};
use ambition_llm::{GenRequest, LlmError, LlmKind, LlmRouter};
