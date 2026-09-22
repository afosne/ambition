//! Ambition engine：use-case 门面（两壳唯一入口，设计见 docs/03-engine-ui）
//! 落库规则：主事务（grader → store）提交后，事件经 `core::EventBus` 广播给旁路
//! （GameProjector → game_state；sync/profile 随后接线），旁路各自幂等、失败不回滚（tracing 记录）。
//! M3：三级判题装配（AiConfig env → Grader）、待裁定桶门面（pending_bucket/resolve_pending）、
//! 成本台账（StoreLedger + cost_summary）、中式英语改错训练（add_chinglish_drills）。

mod ai;
mod ledger;

pub use ai::{AiConfig, LLM_CACHE_TTL_SECS};
pub use ambition_store::ArbitrationRow;
pub use ledger::StoreLedger;

use std::path::Path;
use std::sync::Mutex;

use ambition_core::{Answer, Attempt, DomainEvent, EventBus, GradeResult, Question, Verdict};
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
}

/// 一次提交的产出：判分结果 + 游戏效果
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitOutcome {
    pub grade: GradeResult,
    pub effects: Vec<ambition_gamify::GameEffect>,
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
}

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
        let grader = build_grader(pool.clone(), &ai);
        let (xp, streak) = ambition_store::load_game_state(&pool, "local").await?;
        let engine = Self {
            pool,
            events: Mutex::new(Vec::new()),
            game: GameProjector::hydrate(xp, streak),
            grader,
        };
        engine
            .import_seed(include_str!("../../../data/seed/en/cet4/core.qpack.json"))
            .await?;
        Ok(engine)
    }

    /// 导入内置 seed qpack JSON（幂等：同 qid 跳过；解析失败显式报错不静默）
    pub async fn import_seed(&self, qpack_json: &str) -> Result<PackImportReport, EngineError> {
        let pack = ambition_gen::parse_qpack(qpack_json.as_bytes())?;
        self.import_pack(&pack).await
    }

    /// 导入题包（去重 + 低质拒收，docs/06 §6）
    pub async fn import_pack(
        &self,
        pack: &ambition_gen::QPack,
    ) -> Result<PackImportReport, EngineError> {
        let existing = ambition_store::list_questions(&self.pool).await?;
        let seen: std::collections::HashSet<String> =
            existing.into_iter().map(|q| q.qid.0).collect();
        let plan = ambition_gen::plan_import(pack, &seen, ambition_gen::IMPORT_QUALITY_FLOOR);
        for question in &plan.accepted {
            ambition_store::insert_question(&self.pool, question).await?;
        }
        Ok(PackImportReport {
            added: plan.accepted.len(),
            duplicates: plan.duplicates.len(),
            low_quality: plan.low_quality.len(),
        })
    }

    /// 导出题包（标签过滤：空 = 全量；docs/06 §6 export_qpack）
    pub async fn export_pack(
        &self,
        manifest: ambition_gen::Manifest,
        tags: &[String],
    ) -> Result<ambition_gen::QPack, EngineError> {
        let all = ambition_store::list_questions(&self.pool).await?;
        let filtered: Vec<Question> = if tags.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|q| tags.iter().any(|t| q.tags.contains(t)))
                .collect()
        };
        Ok(ambition_gen::build_qpack(&filtered, manifest))
    }

    /// 练习取题（按 qid 升序取 limit 道；0 = 不限）
    pub async fn start_practice(&self, limit: usize) -> Result<Vec<Question>, EngineError> {
        let all = ambition_store::list_questions(&self.pool).await?;
        Ok(if limit == 0 {
            all
        } else {
            all.into_iter().take(limit).collect()
        })
    }

    /// 结构化出题（prompt → llm → 质检回路 → 入库，docs/06 §1 generate 落库段）
    pub async fn generate(
        &self,
        spec: &ambition_gen::GenSpec,
        generator: &ambition_gen::Generator,
    ) -> Result<Vec<Question>, EngineError> {
        let questions = generator.generate(spec).await?;
        for question in &questions {
            ambition_store::insert_question(&self.pool, question).await?;
        }
        Ok(questions)
    }

    /// 注入题目（手动补题入口；seed/qpack 走 import_*）
    pub async fn add_question(&self, question: &Question) -> Result<(), EngineError> {
        ambition_store::insert_question(&self.pool, question).await?;
        Ok(())
    }

    /// 订阅事件旁路（GameProjector 已内建；sync/profile 随后 subscribe）
    fn publish(&self, event: &DomainEvent) {
        self.game.publish(event.clone());
        // 事件缓冲：sync 投影 / profile 增量消费；锁中毒取内层不吞（P-8）
        let mut buffer = match self.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("[warn] events mutex poisoned, recovering");
                poisoned.into_inner()
            }
        };
        buffer.push(event.clone());
    }

    /// 提交作答：三级判题（规则 → Jev → 桶）→ 单事务落库 → 事件/游戏旁路结算
    pub async fn submit_answer(&self, attempt: &Attempt) -> Result<SubmitOutcome, EngineError> {
        let question = ambition_store::get_question(&self.pool, &attempt.qid)
            .await?
            .ok_or_else(|| EngineError::NotFound(attempt.qid.0.clone()))?;
        // 永不 Err：离线/灰区 → Pending（verdict=pending 同事务进 arbitration 桶）
        let grade = self.grader.grade(&question, &attempt.answer).await;
        ambition_store::insert_attempt_tx(&self.pool, attempt, &grade).await?;
        let event = DomainEvent::AttemptGraded {
            attempt_id: attempt.attempt_id.clone(),
            qid: attempt.qid.clone(),
            verdict: grade.verdict,
            correct: matches!(grade.verdict, Verdict::Correct | Verdict::Acceptable),
        };
        let effects = self.settle(event).await;
        Ok(SubmitOutcome { grade, effects })
    }

    /// 旁路结算：事件广播 → 游戏效果取走 → 游戏状态落库（提交与人工裁决共用）
    async fn settle(&self, event: DomainEvent) -> Vec<ambition_gamify::GameEffect> {
        self.publish(&event);
        let effects = self.game.drain_effects();
        let snapshot = self.game.snapshot();
        if let Err(err) =
            ambition_store::save_game_state(&self.pool, "local", snapshot.xp, snapshot.streak).await
        {
            eprintln!("[warn] game_state persist failed (旁路可补偿): {err}");
            tracing::warn!(error = %err, "game_state persist failed");
        }
        effects
    }

    /// 待裁定桶（pending 列表，docs/03 §2；TUI/画像消费）
    pub async fn pending_bucket(&self) -> Result<Vec<ArbitrationRow>, EngineError> {
        Ok(ambition_store::list_arbitration(&self.pool, "pending").await?)
    }

    /// 人工裁决（y/n）：grades 改判 human + 桶关闭 + 填空可接受答案回流 + 旁路结算
    pub async fn resolve_pending(
        &self,
        attempt_id: &str,
        accept: bool,
    ) -> Result<GradeResult, EngineError> {
        let row = ambition_store::get_arbitration(&self.pool, attempt_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("arbitration:{attempt_id}")))?;
        let Some(grade) =
            ambition_store::resolve_arbitration(&self.pool, attempt_id, accept).await?
        else {
            return Err(EngineError::NotFound(format!("arbitration:{attempt_id}")));
        };
        // 裁决回流：人工认可的填空答案进可接受表（同类题下次规则直判，docs/07 §4）
        if accept && let Answer::Fill(values) = &row.answer {
            let qid = ambition_core::Qid(row.qid.clone());
            for (index, value) in values.iter().enumerate() {
                match ambition_store::add_blank_acceptable(&self.pool, &qid, index, value).await {
                    Ok(true) => {
                        tracing::info!(%index, %value, "arbitration refill → acceptable table")
                    }
                    Ok(false) => {}
                    Err(err) => {
                        tracing::warn!(error = %err, "acceptable refill failed (裁决已生效)")
                    }
                }
            }
        }
        let event = DomainEvent::AttemptGraded {
            attempt_id: row.attempt_id.clone(),
            qid: ambition_core::Qid(row.qid),
            verdict: grade.verdict,
            correct: matches!(grade.verdict, Verdict::Correct | Verdict::Acceptable),
        };
        self.settle(event).await;
        Ok(grade)
    }

    /// 成本台账门面：区间 [from_unix, to_unix) 按 kind 聚合（llm_usage；docs/05 §1）
    pub async fn cost_summary(
        &self,
        from_unix: i64,
        to_unix: i64,
    ) -> Result<ambition_core::UsageSummary, EngineError> {
        Ok(ambition_store::usage_summary(&self.pool, from_unix, to_unix).await?)
    }

    /// 中式英语改错训练题注入题库（data/chinglish；limit=0 全量；同 qid 幂等覆盖）
    pub async fn add_chinglish_drills(&self, limit: usize) -> Result<Vec<Question>, EngineError> {
        let questions = ambition_gen::drill_questions(&ambition_gen::builtin_chinglish(), limit);
        for question in &questions {
            ambition_store::insert_question(&self.pool, question).await?;
        }
        Ok(questions)
    }

    /// 取走并清空事件缓冲（sync 投影 / profile 增量消费）
    pub fn drain_events(&self) -> Vec<DomainEvent> {
        match self.events.lock() {
            Ok(mut buffer) => std::mem::take(&mut *buffer),
            Err(poisoned) => {
                eprintln!("[warn] events mutex poisoned, recovering");
                std::mem::take(&mut *poisoned.into_inner())
            }
        }
    }

    /// 当前游戏状态快照（测试 / TUI 展示）
    pub fn game_state(&self) -> ambition_gamify::GameState {
        self.game.snapshot()
    }
}

/// 按配置装配三级判题器（key 缺席 → 对应级别 None = 离线降级；StoreLedger 经 engine 装配，05 不依赖 store）
fn build_grader(pool: SqlitePool, ai: &AiConfig) -> ambition_grader::Grader {
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
    ambition_grader::Grader::new(jev, llm, ambition_grader::Thresholds::english())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{
        Attempt, AttemptMode, BlankItem, ChoiceBody, FillBody, OptionItem, QuestionKind,
    };

    fn choice_question() -> Question {
        Question::new(
            "She is quick.".into(),
            QuestionKind::SingleChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "quick".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "slow".into(),
                    },
                ],
                answer: vec![0],
            }),
            1,
            vec!["vocab".into()],
            ambition_core::Provenance::Seed,
        )
    }

    fn attempt_for(qid: ambition_core::Qid, selected: Vec<u32>, id: &str) -> Attempt {
        Attempt {
            attempt_id: id.into(),
            qid,
            answer: Answer::Choice(selected),
            timing: None,
            self_confidence: None,
            mode: AttemptMode::Practice,
        }
    }

    fn tmp_db_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ambition-engine-{tag}-{}.db", std::process::id()))
    }

    #[test]
    fn test_submit_outcome_carries_grade_and_effects() {
        let outcome = SubmitOutcome {
            grade: GradeResult {
                verdict: Verdict::Correct,
                score: 1.0,
                max: 1.0,
                confidence: 1.0,
                graded_by: ambition_core::GradedBy::Rule,
                items: vec![],
                error_tags: vec![],
            },
            effects: vec![ambition_gamify::GameEffect::Xp(2)],
        };
        assert_eq!(outcome.grade.verdict, Verdict::Correct);
        assert_eq!(outcome.effects.len(), 1);
    }

    #[test]
    fn test_pack_import_report_defaults_zero() {
        let report = PackImportReport::default();
        assert_eq!(report.added, 0);
        assert_eq!(report.duplicates, 0);
        assert_eq!(report.low_quality, 0);
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
            .import_seed(include_str!("../../../data/seed/en/cet4/core.qpack.json"))
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
    async fn test_submit_answer_grades_and_persists_game_effects() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_question();
        engine.add_question(&q).await.expect("注入");

        let ok = engine
            .submit_answer(&attempt_for(q.qid.clone(), vec![0], "a-1"))
            .await
            .expect("正确提交");
        assert_eq!(ok.grade.verdict, Verdict::Correct);
        assert_eq!(
            ok.effects,
            vec![
                ambition_gamify::GameEffect::Xp(2),
                ambition_gamify::GameEffect::ComboUp
            ]
        );
        // 改进 #1：效果已落库
        let (xp, streak) = ambition_store::load_game_state(&engine.pool, "local")
            .await
            .expect("读档");
        assert_eq!((xp, streak), (2, 1), "game_state 落库");

        let wrong = engine
            .submit_answer(&attempt_for(q.qid.clone(), vec![1], "a-2"))
            .await
            .expect("错误提交");
        assert_eq!(wrong.grade.verdict, Verdict::Wrong);
        let (xp2, streak2) = ambition_store::load_game_state(&engine.pool, "local")
            .await
            .expect("读档");
        assert_eq!((xp2, streak2), (3, 0), "答错 +1xp 断连击");

        let events = engine.drain_events();
        assert_eq!(events.len(), 2);
        assert!(engine.drain_events().is_empty(), "取走即清空");
    }

    #[tokio::test]
    async fn test_submit_answer_rejects_missing_and_rule_gap() {
        let engine = Engine::open_memory().await.expect("打开");
        let missing = engine
            .submit_answer(&attempt_for(
                ambition_core::Qid("nope".into()),
                vec![0],
                "x",
            ))
            .await;
        assert!(matches!(missing, Err(EngineError::NotFound(_))));

        // 多选漏选：规则层不判 → RuleIncomplete（M3 交 Jev）
        let q = Question::new(
            "pick two".into(),
            QuestionKind::MultiChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "a".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "b".into(),
                    },
                    OptionItem {
                        id: 2,
                        text: "c".into(),
                    },
                ],
                answer: vec![0, 2],
            }),
            2,
            vec![],
            ambition_core::Provenance::Seed,
        );
        engine.add_question(&q).await.expect("注入");
        let partial = engine
            .submit_answer(&attempt_for(q.qid.clone(), vec![0], "a-partial"))
            .await
            .expect("漏选离线不再报错（M3：进待裁定桶）");
        assert_eq!(
            partial.grade.verdict,
            Verdict::Pending,
            "离线漏选 → Pending"
        );
        assert_eq!(partial.grade.confidence, 0.0, "低置信标记");
        let bucket = engine.pending_bucket().await.expect("桶");
        assert_eq!(bucket.len(), 1, "Pending 同事务进桶");
        assert_eq!(bucket[0].attempt_id, "a-partial");
        assert_eq!(bucket[0].qid, q.qid.0);

        // 填空规则层全对
        let fill_q = Question::new(
            "She is ___ student.".into(),
            QuestionKind::FillBlank(FillBody {
                sentence: "She is ___ student.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["a".into()],
                }],
            }),
            1,
            vec![],
            ambition_core::Provenance::Seed,
        );
        engine.add_question(&fill_q).await.expect("注入填空");
        let fill_attempt = Attempt {
            attempt_id: "a-fill".into(),
            qid: fill_q.qid,
            answer: Answer::Fill(vec!["a".into()]),
            timing: None,
            self_confidence: None,
            mode: AttemptMode::Practice,
        };
        let out = engine.submit_answer(&fill_attempt).await.expect("填空提交");
        assert_eq!(out.grade.verdict, Verdict::Correct);
    }

    #[tokio::test]
    async fn test_import_pack_dedupes_and_rejects_low_quality() {
        let engine = Engine::open_memory().await.expect("打开");
        let seed_count = engine.start_practice(0).await.expect("取题").len();

        let report = engine
            .import_seed(include_str!("../../../data/seed/en/cet4/core.qpack.json"))
            .await
            .expect("再导 seed");
        assert_eq!(report.added, 0);
        assert_eq!(report.duplicates, seed_count);

        // 新题（不同内容）低质 → 拒收；seed 题重复 → 去重
        let mut low = Question::new(
            "Low quality stem.".into(),
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
            vec!["vocab".into()],
            ambition_core::Provenance::Generated,
        );
        low.quality_score = 0.1;
        let seed_dup = engine
            .start_practice(1)
            .await
            .expect("取一道 seed")
            .remove(0);
        let pack =
            ambition_gen::build_qpack(&[low, seed_dup], ambition_gen::Manifest::local("t", "cet4"));
        let report2 = engine.import_pack(&pack).await.expect("导入");
        assert_eq!(report2.low_quality, 1, "低质题拒收");
        assert_eq!(report2.duplicates, 1, "已存在题去重");
        assert_eq!(report2.added, 0, "无新增");
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            seed_count,
            "库未变"
        );
    }

    #[tokio::test]
    async fn test_export_pack_filters_by_tags_and_roundtrips() {
        let engine = Engine::open_memory().await.expect("打开");
        let all = engine.start_practice(0).await.expect("取题");
        let full = engine
            .export_pack(ambition_gen::Manifest::local("exp", "cet4"), &[])
            .await
            .expect("全量导出");
        assert_eq!(full.questions.len(), all.len());

        let bytes = serde_json::to_vec(&full).expect("序列化");
        let parsed = ambition_gen::parse_qpack(&bytes).expect("解析（防篡改通过）");
        assert_eq!(parsed.questions.len(), all.len());

        // 标签过滤：不存在的标签 → 空包
        let none = engine
            .export_pack(
                ambition_gen::Manifest::local("exp", "cet4"),
                &["no-such-tag".to_string()],
            )
            .await
            .expect("过滤导出");
        assert!(none.questions.is_empty(), "标签过滤生效");
    }

    #[tokio::test]
    async fn test_start_practice_respects_limit() {
        let engine = Engine::open_memory().await.expect("打开");
        let all = engine.start_practice(0).await.expect("取题");
        let capped = engine.start_practice(1).await.expect("取题");
        assert_eq!(capped.len(), 1.min(all.len()), "limit 生效");
        assert_eq!(capped[0].qid, all[0].qid, "保持 qid 升序取前 N");
    }

    #[tokio::test]
    async fn test_add_question_is_idempotent() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_question();
        engine.add_question(&q).await.expect("注入");
        engine.add_question(&q).await.expect("幂等注入");
        let out = engine
            .submit_answer(&attempt_for(q.qid, vec![0], "a-ok"))
            .await
            .expect("提交");
        assert_eq!(out.grade.verdict, Verdict::Correct);
    }

    #[tokio::test]
    async fn test_drain_events_returns_buffered_then_empty() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_question();
        engine.add_question(&q).await.expect("注入");
        engine
            .submit_answer(&attempt_for(q.qid, vec![0], "a-drain"))
            .await
            .expect("提交");
        assert_eq!(engine.drain_events().len(), 1);
        assert!(engine.drain_events().is_empty(), "取走即清空");
    }

    #[tokio::test]
    async fn test_assemble_hydrates_game_and_imports_seed() {
        let engine = Engine::open_memory().await.expect("assemble 路径");
        assert_eq!(
            engine.game_state(),
            ambition_gamify::GameState { xp: 0, streak: 0 },
            "内存库无存档 = 零值"
        );
        assert!(!engine.start_practice(0).await.expect("seed 题").is_empty());
    }

    #[tokio::test]
    async fn test_game_state_reflects_persisted_progress() {
        let engine = Engine::open_memory().await.expect("打开");
        assert_eq!(
            engine.game_state(),
            ambition_gamify::GameState { xp: 0, streak: 0 },
            "内存库零值"
        );
        let q = choice_question();
        engine.add_question(&q).await.expect("注入");
        engine
            .submit_answer(&attempt_for(q.qid, vec![0], "a-gs"))
            .await
            .expect("提交");
        assert_eq!(
            engine.game_state(),
            ambition_gamify::GameState { xp: 2, streak: 1 },
            "提交后投影器状态"
        );
    }

    #[tokio::test]
    async fn test_publish_buffers_event_for_bypass_consumers() {
        let engine = Engine::open_memory().await.expect("打开");
        engine.publish(&DomainEvent::AttemptGraded {
            attempt_id: "pub-1".into(),
            qid: ambition_core::Qid("q".into()),
            verdict: ambition_core::Verdict::Correct,
            correct: true,
        });
        assert_eq!(engine.drain_events().len(), 1, "publish 进缓冲");
        assert_eq!(engine.game_state().xp, 2, "publish 同时驱动游戏投影器");
    }

    #[tokio::test]
    async fn test_generate_persists_generated_questions() {
        use ambition_gen::{GenSpec, Generator, QuestionKindTag};
        use ambition_jev::JevClient;
        use ambition_llm::{LlmRouter, MemLedger, OpenAiCompat};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let llm_server = MockServer::start().await;
        let jev_server = MockServer::start().await;
        let draft = serde_json::json!({
            "questions": [{
                "kind": "single_choice",
                "stem": "Water ___ at 100 degrees Celsius.",
                "options": ["boil", "boils", "boiling", "boiled"],
                "answers": ["boils"],
                "difficulty": 1,
                "tags": ["science"]
            }]
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": draft.to_string()}}],
                "usage": {"prompt_tokens": 40, "completion_tokens": 30}
            })))
            .mount(&llm_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "model": "jev-1.13.0",
                "answers": {
                    "key_correct": {"type": "noul", "noul": 0.97},
                    "natural": {"type": "score", "score": 1.9, "confidence": 0.9},
                    "difficulty_fit": {"type": "score", "score": 1.8, "confidence": 0.8},
                    "unique_answer": {"type": "noul", "noul": 0.93},
                    "distractor_valid": {"type": "noul", "noul": 0.88}
                },
                "usage": {"input_tokens": 180, "output_tokens": 25}
            })))
            .mount(&jev_server)
            .await;

        let router = LlmRouter::new(
            Box::new(OpenAiCompat::new(
                format!("{}/v1", llm_server.uri()),
                "test-model",
                Some("k".into()),
            )),
            None,
            Box::new(MemLedger::new()),
        );
        let generator = Generator::new(
            router,
            JevClient::with_options(&jev_server.uri(), "jev-latest", Some("j".into()), None),
        );

        let engine = Engine::open_memory().await.expect("打开");
        let before = engine.start_practice(0).await.expect("取题").len();
        let spec = GenSpec::new("cet4", vec![QuestionKindTag::SingleChoice], 1);
        let questions = engine.generate(&spec, &generator).await.expect("生成");
        assert_eq!(questions.len(), 1);
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            before + 1,
            "生成题已入库"
        );
        // 生成 Offline 错误路径：无 key 显式报错
        let router2 = LlmRouter::new(
            Box::new(OpenAiCompat::new(
                format!("{}/v1", llm_server.uri()),
                "m",
                None,
            )),
            None,
            Box::new(MemLedger::new()),
        );
        let offline = Generator::new(
            router2,
            JevClient::with_options(&jev_server.uri(), "jev-latest", None, None),
        );
        let err = engine.generate(&spec, &offline).await.expect_err("无 key");
        assert!(
            matches!(err, EngineError::Gen(_)),
            "无 key → Gen 错误: {err}"
        );
    }

    #[tokio::test]
    async fn test_import_seed_rejects_corrupt_json_explicitly() {
        // 显式错误路径（P-8：坏 seed 不静默）——直接测 import_seed 的解析失败分支
        let engine = Engine::open_memory().await.expect("打开");
        let err = engine
            .import_seed("not a qpack")
            .await
            .expect_err("坏 seed");
        assert!(
            matches!(err, EngineError::Gen(_)),
            "坏 seed → Gen 错误: {err}"
        );
    }

    fn multi_pick_two() -> Question {
        Question::new(
            "pick two".into(),
            QuestionKind::MultiChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "a".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "b".into(),
                    },
                    OptionItem {
                        id: 2,
                        text: "c".into(),
                    },
                ],
                answer: vec![0, 2],
            }),
            2,
            vec![],
            ambition_core::Provenance::Seed,
        )
    }

    #[tokio::test]
    async fn test_pending_bucket_lists_only_pending_entries() {
        let engine = Engine::open_memory().await.expect("打开");
        assert!(
            engine.pending_bucket().await.expect("桶").is_empty(),
            "初始空桶"
        );
        // 规则可判的提交不进桶
        let q = choice_question();
        engine.add_question(&q).await.expect("注入");
        engine
            .submit_answer(&attempt_for(q.qid.clone(), vec![0], "pb-ok"))
            .await
            .expect("提交");
        assert!(
            engine.pending_bucket().await.expect("桶").is_empty(),
            "定判不进桶"
        );
        // 离线漏选 → Pending 进桶
        let m = multi_pick_two();
        engine.add_question(&m).await.expect("注入");
        engine
            .submit_answer(&attempt_for(m.qid.clone(), vec![0], "pb-pending"))
            .await
            .expect("提交");
        let bucket = engine.pending_bucket().await.expect("桶");
        assert_eq!(bucket.len(), 1);
        assert_eq!(bucket[0].attempt_id, "pb-pending");
        assert_eq!(bucket[0].reason, "pipeline_undecided");
        assert!(matches!(
            &bucket[0].answer,
            ambition_core::Answer::Choice(ids) if ids.as_slice() == [0]
        ));
    }

    #[tokio::test]
    async fn test_resolve_pending_accept_and_reject_update_grade_and_game() {
        let engine = Engine::open_memory().await.expect("打开");
        let m = multi_pick_two();
        engine.add_question(&m).await.expect("注入");
        engine
            .submit_answer(&attempt_for(m.qid.clone(), vec![0], "rp-1"))
            .await
            .expect("提交");
        engine
            .submit_answer(&attempt_for(m.qid.clone(), vec![2], "rp-2"))
            .await
            .expect("提交");
        assert_eq!(engine.pending_bucket().await.expect("桶").len(), 2);

        // accept：human 判对 + 满分 + 旁路结算（Pending 提交 +1xp，裁决正确 +2xp）
        let accepted = engine.resolve_pending("rp-1", true).await.expect("裁决");
        assert_eq!(accepted.verdict, Verdict::Correct);
        assert_eq!(accepted.graded_by, ambition_core::GradedBy::Human);
        assert_eq!(accepted.score, accepted.max, "accept → 满分");
        assert_eq!(accepted.confidence, 1.0, "人工权威");
        // reject：wrong + 零分
        let rejected = engine.resolve_pending("rp-2", false).await.expect("裁决");
        assert_eq!(rejected.verdict, Verdict::Wrong);
        assert_eq!(rejected.score, 0.0);
        assert_eq!(rejected.graded_by, ambition_core::GradedBy::Human);
        // 桶清空 + 游戏状态经 settle 落库
        assert!(
            engine.pending_bucket().await.expect("桶").is_empty(),
            "裁决后出桶"
        );
        let state = engine.game_state();
        assert_eq!(state.xp, 1 + 2, "两次 Pending 提交 + 一次 accept 结算");
        let (xp, _) = ambition_store::load_game_state(&engine.pool, "local")
            .await
            .expect("读档");
        assert_eq!(xp, state.xp, "settle 落库");
        // 不存在 / 已裁决 → NotFound 显式报错
        assert!(matches!(
            engine.resolve_pending("nope", true).await,
            Err(EngineError::NotFound(_))
        ));
        let repeat = engine.resolve_pending("rp-1", false).await;
        assert!(
            matches!(repeat, Err(EngineError::NotFound(_))),
            "重复裁决不静默"
        );
    }

    #[tokio::test]
    async fn test_resolve_pending_refills_acceptable_for_rule_regrade() {
        // Jev 灰区 → Pending → 人工 accept → 回流 → 二次作答规则直判闭环（docs/07 §7）
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {"acceptable": {"type": "noul", "noul": 0.5}}
            })))
            .mount(&server)
            .await;
        let ai = AiConfig {
            jev_base: format!("{}/v1", server.uri()),
            jev_key: Some("k".into()),
            ..AiConfig::offline()
        };
        let engine = Engine::open_memory_with_ai(ai).await.expect("打开");
        let fill_q = Question::new(
            "She ___ a student.".into(),
            QuestionKind::FillBlank(FillBody {
                sentence: "She ___ a student.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["is".into()],
                }],
            }),
            2,
            vec!["grammar".into()],
            ambition_core::Provenance::Seed,
        );
        engine.add_question(&fill_q).await.expect("注入");
        let attempt = Attempt {
            attempt_id: "refill-1".into(),
            qid: fill_q.qid.clone(),
            answer: Answer::Fill(vec!["be".into()]),
            timing: None,
            self_confidence: None,
            mode: ambition_core::AttemptMode::Practice,
        };
        let first = engine.submit_answer(&attempt).await.expect("提交");
        assert_eq!(first.grade.verdict, Verdict::Pending, "灰区 → 桶");
        let resolved = engine
            .resolve_pending("refill-1", true)
            .await
            .expect("裁决");
        assert_eq!(resolved.verdict, Verdict::Correct);
        // 回流后：同一答案二次作答规则直判
        let again = Attempt {
            attempt_id: "refill-2".into(),
            ..attempt.clone()
        };
        let second = engine.submit_answer(&again).await.expect("再提交");
        assert_eq!(second.grade.verdict, Verdict::Correct, "回流生效");
        assert_eq!(
            second.grade.graded_by,
            ambition_core::GradedBy::Rule,
            "同类题规则直判（0 成本）"
        );
        // 回流后的可接受表可读回
        let got = ambition_store::get_question(&engine.pool, &fill_q.qid)
            .await
            .expect("读题")
            .expect("存在");
        let QuestionKind::FillBlank(fill) = got.kind else {
            panic!("填空")
        };
        assert_eq!(fill.blanks[0].acceptable, vec!["is", "be"]);
    }

    #[tokio::test]
    async fn test_cost_summary_aggregates_llm_usage_rows() {
        let engine = Engine::open_memory().await.expect("打开");
        ambition_store::record_usage(&engine.pool, "question_gen", "m1", 100, 20)
            .await
            .expect("记账");
        ambition_store::record_usage(&engine.pool, "jev", "jev-latest", 50, 10)
            .await
            .expect("记账");
        let summary = engine.cost_summary(0, i64::MAX).await.expect("台账");
        assert_eq!(summary.rows.len(), 2, "按 kind 聚合");
        assert_eq!((summary.tokens_in, summary.tokens_out), (150, 30));
        let future = engine
            .cost_summary(i64::MAX - 1, i64::MAX)
            .await
            .expect("台账");
        assert_eq!(future.rows.len(), 0, "区间外为空");
    }

    #[tokio::test]
    async fn test_add_chinglish_drills_inserts_idempotently() {
        let engine = Engine::open_memory().await.expect("打开");
        let before = engine.start_practice(0).await.expect("取题").len();
        let three = engine.add_chinglish_drills(3).await.expect("注入");
        assert_eq!(three.len(), 3);
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            before + 3,
            "入题库"
        );
        // 同 qid 幂等覆盖
        engine.add_chinglish_drills(3).await.expect("再注入");
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            before + 3,
            "重注入不重复"
        );
        // limit=0 全量
        let all = engine.add_chinglish_drills(0).await.expect("全量");
        assert!(all.len() >= 10, "内置错式 ≥10: {}", all.len());
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            before + all.len(),
            "全量入库后计数"
        );
        // 训练题可作答：答案恒为地道改写
        let drill = all[0].clone();
        let QuestionKind::SingleChoice(body) = drill.kind.clone() else {
            panic!("单选改错")
        };
        engine.add_question(&drill).await.expect("注入");
        let out = engine
            .submit_answer(&attempt_for(drill.qid, body.answer, "drill-1"))
            .await
            .expect("提交");
        assert_eq!(out.grade.verdict, Verdict::Correct, "改错题可判");
    }

    #[tokio::test]
    async fn test_settle_publishes_buffers_and_persists_game_state() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_question();
        let event = DomainEvent::AttemptGraded {
            attempt_id: "settle-1".into(),
            qid: q.qid,
            verdict: Verdict::Correct,
            correct: true,
        };
        let effects = engine.settle(event).await;
        assert!(!effects.is_empty(), "效果已取走");
        assert_eq!(engine.drain_events().len(), 1, "事件进缓冲");
        let (xp, streak) = ambition_store::load_game_state(&engine.pool, "local")
            .await
            .expect("读档");
        assert_eq!((xp, streak), (2, 1), "settle 落库");
        assert!(engine.drain_events().is_empty(), "取走即清空");
    }

    #[tokio::test]
    async fn test_build_grader_offline_keeps_rule_path() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("内存库");
        ambition_store::init_db(&pool).await.expect("建表");
        let grader = build_grader(pool, &AiConfig::offline());
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
