//! Ambition engine：use-case 门面（两壳唯一入口，设计见 docs/03-engine-ui）
//! 落库规则：主事务（grader → store）提交后，事件广播给旁路（gamify/sync/profile）。

use std::sync::Mutex;

use ambition_core::{Answer, Attempt, DomainEvent, GradeResult, Question, Verdict};
use ambition_gamify::{GameEffect, on_event};
use ambition_grader::grade_rule;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

/// 引擎错误（外部失败可恢复，docs/03-engine-ui §错误与验证）
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("store: {0}")]
    Store(#[from] sqlx::Error),
    #[error("question not found: {0}")]
    NotFound(String),
    #[error("rule layer cannot decide this answer (需要 Jev 升级，M3)")]
    RuleIncomplete,
}

/// 一次提交的产出：判分结果 + 游戏效果
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitOutcome {
    pub grade: GradeResult,
    pub effects: Vec<GameEffect>,
}

/// 引擎实例：业务库连接 + 事件缓冲（旁路消费后取走）
pub struct Engine {
    pool: SqlitePool,
    events: Mutex<Vec<DomainEvent>>,
}

impl Engine {
    /// 打开内存库（M1 单机/测试形态；文件库路径版在 M2 接入 config）
    pub async fn open_memory() -> Result<Self, EngineError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        ambition_store::init_db(&pool).await?;
        Ok(Self {
            pool,
            events: Mutex::new(Vec::new()),
        })
    }

    /// 注入题目（内置 seed / qpack 导入入口，M1 手动）
    pub async fn add_question(&self, question: &Question) -> Result<(), EngineError> {
        ambition_store::insert_question(&self.pool, question).await?;
        Ok(())
    }

    /// 提交作答：规则判题 → 单事务落库 → 事件广播（gamify 效果一并返回）
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
        let effects = on_event(&event);
        // 事件缓冲：旁路（gamify/sync/profile）消费后取走；锁中毒按空处理（尽力而为）
        if let Ok(mut buffer) = self.events.lock() {
            buffer.push(event);
        }
        Ok(SubmitOutcome { grade, effects })
    }

    /// 取走并清空事件缓冲（sync 投影 / profile 增量消费）
    pub fn drain_events(&self) -> Vec<DomainEvent> {
        self.events
            .lock()
            .map(|mut buffer| std::mem::take(&mut *buffer))
            .unwrap_or_default()
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

    #[test]
    fn test_open_memory_creates_business_schema() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let engine = Engine::open_memory().await.expect("打开内存库");
                let count: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE name = 'grades'")
                        .fetch_one(&engine.pool)
                        .await
                        .expect("查 schema");
                assert_eq!(count, 1);
            });
    }

    #[test]
    fn test_add_question_then_readable_by_engine_flow() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let engine = Engine::open_memory().await.expect("打开");
                let q = choice_question();
                engine.add_question(&q).await.expect("注入题目");
                // 再次注入幂等（OR REPLACE），提交作答仍工作
                engine.add_question(&q).await.expect("幂等注入");
                let out = engine
                    .submit_answer(&attempt_for(q.qid, vec![0], "a-ok"))
                    .await
                    .expect("提交");
                assert_eq!(out.grade.verdict, Verdict::Correct);
            });
    }

    #[test]
    fn test_submit_answer_grades_and_emits_effects() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let engine = Engine::open_memory().await.expect("打开");
                let q = choice_question();
                engine.add_question(&q).await.expect("注入");

                let ok = engine
                    .submit_answer(&attempt_for(q.qid.clone(), vec![0], "a-1"))
                    .await
                    .expect("正确提交");
                assert_eq!(ok.grade.verdict, Verdict::Correct);
                assert_eq!(ok.effects, vec![GameEffect::Xp(2), GameEffect::ComboUp]);

                let wrong = engine
                    .submit_answer(&attempt_for(q.qid.clone(), vec![1], "a-2"))
                    .await
                    .expect("错误提交");
                assert_eq!(wrong.grade.verdict, Verdict::Wrong);
                assert_eq!(
                    wrong.effects,
                    vec![GameEffect::Xp(1), GameEffect::ComboBreak]
                );

                let events = engine.drain_events();
                assert_eq!(events.len(), 2);
                assert!(engine.drain_events().is_empty(), "取走即清空");
            });
    }

    #[test]
    fn test_drain_events_returns_buffered_then_empty() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let engine = Engine::open_memory().await.expect("打开");
                let q = choice_question();
                engine.add_question(&q).await.expect("注入");
                engine
                    .submit_answer(&attempt_for(q.qid, vec![0], "a-drain"))
                    .await
                    .expect("提交");
                assert_eq!(engine.drain_events().len(), 1);
                assert!(engine.drain_events().is_empty(), "取走即清空");
            });
    }

    #[test]
    fn test_submit_answer_rejects_missing_and_rule_gap() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
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
                );
                engine.add_question(&q).await.expect("注入");
                let partial = engine
                    .submit_answer(&attempt_for(q.qid, vec![0], "a-partial"))
                    .await;
                assert!(matches!(partial, Err(EngineError::RuleIncomplete)));

                // 未覆盖题型（空填空题体不支持内容缺失场景由 grader 单测覆盖）
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
            });
    }
}
