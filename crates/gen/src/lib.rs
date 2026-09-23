//! Ambition gen：出题生成 + Jev 质检回路 + qpack（docs/06-generation，M2 落地范围）
//! 主流程：prompt → llm.structured（JSON Schema 约束草稿）→ 校验（qid=内容哈希）
//! → qc_question（Jev fan-out 单请求）→ 不合格重生成 ≤2 轮 → 交 engine 入库。

mod chinglish;

mod qpack;

// 模块分层（docs/01 §组织：lib.rs 薄壳，文件 ≤500 行，pre-commit 门禁强制）
mod draft;
mod generator;
mod prompt;
mod qc;
mod spec;

pub use draft::*;
pub use generator::*;
pub(crate) use prompt::*;
pub use qc::*;
pub use spec::*;

pub use chinglish::{
    CHINGLISH_SCHEMA_VERSION, ChinglishItem, builtin_chinglish, drill_questions, parse_chinglish,
};
pub use qpack::{
    IMPORT_QUALITY_FLOOR, ImportPlan, Manifest, PackItem, QPACK_SCHEMA_VERSION, QPack, build_qpack,
    parse_qpack, plan_import,
};

use ambition_jev::{JevClient, JevError, JevQuestion, JevQuestions, NoulQ, ScoreQ};
use ambition_llm::{GenRequest, JsonSchema, LlmError, LlmKind, LlmRouter};
