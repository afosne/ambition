//! Ambition engine：use-case 门面（两壳唯一入口，设计见 docs/03-engine-ui）
//! 落库规则：主事务（grader → store）提交后，事件经 `core::EventBus` 广播给旁路
//! （GameProjector → game_state；sync/profile 随后接线），旁路各自幂等、失败不回滚（tracing 记录）。
//! M3：三级判题装配（AiConfig env → Grader）、待裁定桶门面（pending_bucket/resolve_pending）、
//! 成本台账（StoreLedger + cost_summary）、中式英语改错训练（add_chinglish_drills）。

mod assembly;
#[cfg(test)]
mod common;
mod essay;
mod paper;
mod practice;
mod scaffold;
mod srs;
// 各模块只承载 impl 块与 crate 内部 fns——impl 自动生效，无项可再导出
// 模块分层（docs/01 §组织：lib.rs 薄壳 + 细分功能目录 assembly/practice/srs，文件 ≤500 行，pre-commit 门禁强制）

/// SRS 关键类型再导出（两壳经 engine 消费，避免直接依赖 srs crate）
pub use ambition_srs::{CardAction, Rating};
pub use ambition_store::ArbitrationRow;
pub use assembly::ai::{AiConfig, LLM_CACHE_TTL_SECS};
pub use assembly::ledger::StoreLedger;
pub use paper::MockSession;
pub use srs::lexicon::{ExamplePair, Pattern, Sense};

use std::path::Path;
use std::sync::Mutex;

use ambition_core::{
    Answer, Attempt, DomainEvent, EventBus, GradeResult, Paper, PaperSection, PaperSpec, Question,
    Verdict,
};
use ambition_gamify::GameProjector;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

/// 引擎错误（外部失败可恢复，docs/03-engine-ui §错误与验证）
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("store: {0}")]
    Store(#[from] sqlx::Error),
    #[error("question not found: {0}")]
    NotFound(String),
    #[error("generation: {0}")]
    Gen(#[from] ambition_gen::GenError),
    #[error("offline: no AI key configured (AMBI_LLM_KEY / AMBI_JEV_KEY)")]
    Offline,
    #[error("seed import failed: {0}")]
    Seed(String),
    #[error("srs: {0}")]
    Srs(#[from] ambition_srs::SrsError),
    #[error("vocab: {0}")]
    Vocab(String),
    #[error("jev: {0}")]
    Jev(#[from] ambition_jev::JevError),
    #[error("llm: {0}")]
    Llm(#[from] ambition_llm::LlmError),
    /// 套卷（组卷不足/规格错——M5-W6）
    #[error("paper: {0}")]
    Paper(String),
}

/// Jev 错误跨层映射（离线 → 配置提示口径；其余带源可见，P-8 不伪造，scaffold/essay 共用）
fn from_jev(err: ambition_jev::JevError) -> EngineError {
    match err {
        ambition_jev::JevError::Offline => EngineError::Offline,
        other => EngineError::Jev(other),
    }
}

/// Llm 错误跨层映射（同 from_jev 口径）
fn from_llm(err: ambition_llm::LlmError) -> EngineError {
    match err {
        ambition_llm::LlmError::Offline => EngineError::Offline,
        other => EngineError::Llm(other),
    }
}

/// 一次提交的产出：判分结果 + 游戏效果 + 入卡处置回传（M4，docs/03 §2）
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitOutcome {
    pub grade: GradeResult,
    pub effects: Vec<ambition_gamify::GameEffect>,
    /// SRS 旁路处置（None = 不入卡：Skip/Pending/旁路失败降级，UI 提示用）
    pub srs: Option<ambition_srs::CardAction>,
}

/// 题包导入结果（docs/06 §6 ImportReport）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PackImportReport {
    pub added: usize,
    pub duplicates: usize,
    pub low_quality: usize,
}

/// 引擎实例：业务库连接 + 事件缓冲（旁路消费后取走）+ 游戏投影器 + 三级判题器
pub struct Engine {
    pool: SqlitePool,
    events: Mutex<Vec<DomainEvent>>,
    game: GameProjector,
    grader: ambition_grader::Grader,
    /// LLM 句柄（annotate/model_essays 直用；与 grader 共享同一 Arc，装配单点，M5-W5）
    llm: Option<std::sync::Arc<ambition_llm::LlmRouter>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_jev_maps_offline_to_config_hint() {
        assert!(
            matches!(
                from_jev(ambition_jev::JevError::Offline),
                EngineError::Offline
            ),
            "离线 → 配置提示"
        );
        let other = ambition_jev::JevError::Invalid("bad".into());
        assert!(matches!(from_jev(other), EngineError::Jev(_)), "其余带源");
    }

    #[test]
    fn test_from_llm_maps_offline_to_config_hint() {
        assert!(
            matches!(
                from_llm(ambition_llm::LlmError::Offline),
                EngineError::Offline
            ),
            "离线 → 配置提示"
        );
        let other = ambition_llm::LlmError::InvalidOutput { attempts: 3 };
        assert!(matches!(from_llm(other), EngineError::Llm(_)), "其余带源");
    }

    #[test]
    fn test_submit_outcome_carries_grade_effects_and_srs() {
        let outcome = SubmitOutcome {
            grade: GradeResult {
                verdict: Verdict::Correct,
                score: 1.0,
                max: 1.0,
                confidence: 1.0,
                graded_by: ambition_core::GradedBy::Rule,
                detail: ambition_core::GradeDetail::Plain,
                error_tags: vec![],
            },
            effects: vec![ambition_gamify::GameEffect::Xp(2)],
            srs: None,
        };
        assert_eq!(outcome.grade.verdict, Verdict::Correct);
        assert_eq!(outcome.effects.len(), 1);
        assert!(
            outcome.srs.is_none(),
            "srs 处置可空（Skip/Pending/旁路失败）"
        );
    }

    #[test]
    fn test_pack_import_report_defaults_zero() {
        let report = PackImportReport::default();
        assert_eq!(report.added, 0);
        assert_eq!(report.duplicates, 0);
        assert_eq!(report.low_quality, 0);
    }
}
