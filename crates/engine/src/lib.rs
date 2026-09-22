//! Ambition engine：use-case 门面（两壳唯一入口，设计见 docs/03-engine-ui）
//! 落库规则：主事务（grader → store）提交后，事件经 `core::EventBus` 广播给旁路
//! （GameProjector → game_state；sync/profile 随后接线），旁路各自幂等、失败不回滚（tracing 记录）。

use std::path::Path;
use std::sync::Mutex;

use ambition_core::{Answer, Attempt, DomainEvent, EventBus, GradeResult, Question, Verdict};
use ambition_gamify::GameProjector;
use ambition_grader::grade_rule;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

/// 引擎错误（外部失败可恢复，docs/03-engine-ui §错误与验证）
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("store: {0}")]
    Store(#[from] sqlx::Error),
    #[error("question not found: {0}")]
    NotFound(String),
    #[error("rule layer cannot decide this answer (需要 Jev 升级，M3)")]
    RuleIncomplete,
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

/// 引擎实例：业务库连接 + 事件缓冲（旁路消费后取走）+ 游戏投影器
pub struct Engine {
    pool: SqlitePool,
    events: Mutex<Vec<DomainEvent>>,
    game: GameProjector,
}

impl Engine {
    /// 打开内存库（测试形态）；导入内置 seed（幂等）
    pub async fn open_memory() -> Result<Self, EngineError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        ambition_store::init_db(&pool).await?;
        let engine = Self::assemble(pool).await?;
        Ok(engine)
    }

    /// 打开文件库（`~/.ambition/ambition.db` 形态，WAL + busy_timeout，docs/02 §2）；seed 幂等导入
    pub async fn open(path: &Path) -> Result<Self, EngineError> {
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
        Self::assemble(pool).await
    }

    /// 装配：建表后 → 水合游戏状态 → seed 幂等导入（内容三级保障①）
    async fn assemble(pool: SqlitePool) -> Result<Self, EngineError> {
        let (xp, streak) = ambition_store::load_game_state(&pool, "local").await?;
        let engine = Self {
            pool,
            events: Mutex::new(Vec::new()),
            game: GameProjector::hydrate(xp, streak),
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

    /// 提交作答：规则判题 → 单事务落库 → 事件广播 → 游戏状态落库（旁路幂等）
    pub async fn submit_answer(&self, attempt: &Attempt) -> Result<SubmitOutcome, EngineError> {
        let question = ambition_store::get_question(&self.pool, &attempt.qid)
            .await?
            .ok_or_else(|| EngineError::NotFound(attempt.qid.0.clone()))?;
        let (answers, fills) = match &attempt.answer {
            Answer::Choice(ids) => (ids.clone(), Vec::new()),
            Answer::Fill(texts) => (Vec::new(), texts.clone()),
        };
        let grade =
            grade_rule(&question.kind, &answers, &fills).ok_or(EngineError::RuleIncomplete)?;
        ambition_store::insert_attempt_tx(&self.pool, attempt, &grade).await?;
        let correct = matches!(grade.verdict, Verdict::Correct | Verdict::Acceptable);
        let event = DomainEvent::AttemptGraded {
            attempt_id: attempt.attempt_id.clone(),
            qid: attempt.qid.clone(),
            verdict: grade.verdict,
            correct,
        };
        self.publish(&event);
        let effects = self.game.drain_effects();
        // 游戏效果落库（改进 #1）：主事务外的旁路，失败可见不回滚
        let snapshot = self.game.snapshot();
        if let Err(err) =
            ambition_store::save_game_state(&self.pool, "local", snapshot.xp, snapshot.streak).await
        {
            eprintln!("[warn] game_state persist failed (旁路可补偿): {err}");
            tracing::warn!(error = %err, "game_state persist failed");
        }
        Ok(SubmitOutcome { grade, effects })
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
            .await;
        assert!(matches!(partial, Err(EngineError::RuleIncomplete)));

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
}
