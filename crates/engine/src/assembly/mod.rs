//! 引擎生命周期与装配（细分域根，docs/03-engine-ui §2）：open/open_memory/open_with_ai/assemble/build_grader（BYOK 注入）
//! 本域含子组件：`assembly::ai`（env → AiConfig）/ `assembly::ledger`（StoreLedger）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

pub mod ai;
pub mod ledger;

use crate::*;

impl Engine {
    /// 打开内存库（测试形态）；导入内置 seed（幂等）；AI 恒离线（需要 AI 走 open_memory_with_ai）
    pub async fn open_memory() -> Result<Self, EngineError> {
        Self::open_memory_with_ai(AiConfig::offline()).await
    }

    /// 打开内存库并装配指定 AI（测试指 base 到 wiremock）
    pub async fn open_memory_with_ai(ai: AiConfig) -> Result<Self, EngineError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        ambition_store::init_db(&pool).await?;
        Self::assemble(pool, ai).await
    }

    /// 打开文件库（`~/.ambition/ambition.db` 形态，WAL + busy_timeout，docs/02 §2）；seed 幂等导入；AI 走 env
    pub async fn open(path: &Path) -> Result<Self, EngineError> {
        Self::open_with_ai(path, AiConfig::from_env()).await
    }

    /// 打开文件库并装配指定 AI（env 覆盖 / 测试注入）
    pub async fn open_with_ai(path: &Path, ai: AiConfig) -> Result<Self, EngineError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| EngineError::Seed(e.to_string()))?;
        }
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let options =
            SqliteConnectOptions::from_str(&url).map_err(|e| EngineError::Seed(e.to_string()))?;
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        // WAL + busy_timeout（docs/02 §2；PRAGMA 对文件库生效）
        sqlx::query("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000")
            .execute(&pool)
            .await?;
        ambition_store::init_db(&pool).await?;
        Self::assemble(pool, ai).await
    }

    /// 装配：缓存 TTL 淘汰 → 判题器 → 水合游戏状态 → seed 幂等导入（内容三级保障①）
    async fn assemble(pool: SqlitePool, ai: AiConfig) -> Result<Self, EngineError> {
        // 缓存淘汰是维护旁路：失败可见不阻断（P-8）
        if let Err(err) = ai::purge_stale_cache(&pool).await {
            tracing::warn!(error = %err, "llm_cache purge failed");
        }
        let (grader, llm) = build_grader(pool.clone(), &ai);
        let (xp, streak) = ambition_store::load_game_state(&pool, "local").await?;
        let game_state = match ambition_store::load_game_profile(&pool, "local").await? {
            Some(raw) => serde_json::from_str::<ambition_gamify::GameState>(&raw).unwrap_or(
                ambition_gamify::GameState {
                    xp,
                    streak,
                    ..Default::default()
                },
            ),
            None => ambition_gamify::GameState {
                xp,
                streak,
                ..Default::default()
            },
        };
        let engine = Self {
            pool,
            events: Mutex::new(Vec::new()),
            game: GameProjector::hydrate_state(game_state),
            grader,
            llm,
            profile: Mutex::new(None),
        };
        engine
            .import_seed(include_str!(
                "../../../../data/seed/en/cet4/core.qpack.json"
            ))
            .await?;
        // M4：内置义项/句式幂等入卡（坏文件仅 warn 不阻断 open，docs/03 §1）
        engine.enroll_builtin_cards().await;
        Ok(engine)
    }
}

/// 按配置装配三级判题器（key 缺席 → 对应级别 None = 离线降级；StoreLedger 经 engine 装配，05 不依赖 store）
fn build_grader(
    pool: SqlitePool,
    ai: &AiConfig,
) -> (
    ambition_grader::Grader,
    Option<std::sync::Arc<ambition_llm::LlmRouter>>,
) {
    use std::sync::Arc;
    let jev = ai.jev_key.as_ref().map(|key| {
        Arc::new(ambition_jev::JevClient::with_options(
            &ai.jev_base,
            ambition_jev::DEFAULT_JEV_MODEL,
            Some(key.clone()),
            Some(Arc::new(StoreLedger::new(pool.clone()))),
        ))
    });
    let llm = ai.llm_key.as_ref().map(|key| {
        Arc::new(ambition_llm::LlmRouter::new(
            Box::new(ambition_llm::OpenAiCompat::new(
                ai.llm_base.clone(),
                ai.llm_model.clone(),
                Some(key.clone()),
            )),
            None,
            Box::new(StoreLedger::new(pool.clone())),
        ))
    });
    let grader =
        ambition_grader::Grader::new(jev, llm.clone(), ambition_grader::Thresholds::english());
    (grader, llm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use ambition_core::{ChoiceBody, OptionItem, QuestionKind};

    fn tmp_db_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ambition-engine-{tag}-{}.db", std::process::id()))
    }

    #[tokio::test]
    async fn test_open_memory_creates_business_schema() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE name = 'grades'")
                .fetch_one(&engine.pool)
                .await
                .expect("查 schema");
        assert_eq!(count, 1);
        let version: i32 = sqlx::query_scalar("SELECT MAX(version) FROM schema_version")
            .fetch_one(&engine.pool)
            .await
            .expect("版本表");
        assert_eq!(version, ambition_store::SCHEMA_VERSION, "迁移已应用");
    }

    #[tokio::test]
    async fn test_open_memory_imports_seed_idempotently() {
        let engine = Engine::open_memory().await.expect("打开");
        let first = engine.start_practice(0).await.expect("取题").len();
        assert!(first > 0, "内置 seed 已导入");
        // 二次导入幂等
        let report = engine
            .import_seed(include_str!(
                "../../../../data/seed/en/cet4/core.qpack.json"
            ))
            .await
            .expect("重导");
        assert_eq!(report.added, 0, "同 qid 全部去重");
        assert_eq!(report.duplicates, first, "重复数 = 已有题数");
        assert_eq!(engine.start_practice(0).await.expect("取题").len(), first);
    }

    #[tokio::test]
    async fn test_open_file_db_roundtrips_and_resumes_game_state() {
        let path = tmp_db_path("file-open");
        let _ = std::fs::remove_file(&path);
        {
            let engine = Engine::open(&path).await.expect("打开文件库");
            assert!(path.exists(), "文件已创建");
            let q = choice_question();
            engine.add_question(&q).await.expect("注入");
            engine
                .submit_answer(&attempt_for(q.qid.clone(), vec![0], "a-file"))
                .await
                .expect("提交");
        }
        // 重开：题在、游戏档续上（assemble 自动水合）
        {
            let engine = Engine::open(&path).await.expect("重开");
            let got = engine
                .start_practice(0)
                .await
                .expect("取题")
                .into_iter()
                .find(|q| q.qid == choice_question().qid);
            assert!(got.is_some(), "题目持久化");
            let state = engine.game_state();
            assert_eq!(state.xp, 2, "重开后 xp 从库续档");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_assemble_hydrates_game_and_imports_seed() {
        let engine = Engine::open_memory().await.expect("assemble 路径");
        assert_eq!(
            (engine.game_state().xp, engine.game_state().streak),
            (0, 0),
            "内存库无存档 = 零值"
        );
        assert!(!engine.start_practice(0).await.expect("seed 题").is_empty());
    }

    #[tokio::test]
    async fn test_build_grader_offline_keeps_rule_path() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("内存库");
        ambition_store::init_db(&pool).await.expect("建表");
        let (grader, llm) = build_grader(pool, &AiConfig::offline());
        assert!(llm.is_none(), "离线无 llm 句柄");
        let m = multi_pick_two();
        let grade = grader.grade(&m, &Answer::Choice(vec![0])).await;
        assert_eq!(grade.verdict, Verdict::Pending, "离线无 AI → 桶");
        let single = Question::new(
            "ok".into(),
            QuestionKind::SingleChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "x".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "y".into(),
                    },
                ],
                answer: vec![0],
            }),
            1,
            vec![],
            ambition_core::Provenance::Seed,
        );
        let rule = grader.grade(&single, &Answer::Choice(vec![0])).await;
        assert_eq!(rule.verdict, Verdict::Correct, "规则直判不受影响");
        assert_eq!(rule.graded_by, ambition_core::GradedBy::Rule);
    }

    #[tokio::test]
    async fn test_open_with_ai_file_db_uses_provided_config() {
        let path = tmp_db_path("file-open-ai");
        let _ = std::fs::remove_file(&path);
        {
            let engine = Engine::open_with_ai(&path, AiConfig::offline())
                .await
                .expect("打开文件库");
            assert!(path.exists(), "文件已创建");
            let q = choice_question();
            engine.add_question(&q).await.expect("注入");
            let out = engine
                .submit_answer(&attempt_for(q.qid, vec![0], "a-open-ai"))
                .await
                .expect("提交");
            assert_eq!(out.grade.verdict, Verdict::Correct);
        }
        // 重开仍可用（seed 幂等 + 配置生效路径）
        {
            let engine = Engine::open_with_ai(&path, AiConfig::offline())
                .await
                .expect("重开");
            assert!(!engine.start_practice(0).await.expect("取题").is_empty());
        }
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_open_memory_with_ai_builds_seed_bank_for_grader() {
        let engine = Engine::open_memory_with_ai(AiConfig::offline())
            .await
            .expect("打开");
        assert!(
            !engine.start_practice(0).await.expect("seed").is_empty(),
            "seed 已导入"
        );
        assert!(engine.pending_bucket().await.expect("桶").is_empty());
        assert_eq!(
            engine
                .cost_summary(0, i64::MAX)
                .await
                .expect("台账")
                .rows
                .len(),
            0,
            "空库零台账"
        );
    }
}
