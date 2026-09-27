//! 练习主流程：start_practice/submit_answer + 事件旁路 publish/settle/drain_events/game_state（docs/03 §4）

use super::*;

impl Engine {
    /// 练习取题（按 qid 升序取 limit 道；0 = 不限）
    pub async fn start_practice(&self, limit: usize) -> Result<Vec<Question>, EngineError> {
        let all = ambition_store::list_questions(&self.pool).await?;
        Ok(if limit == 0 {
            all
        } else {
            all.into_iter().take(limit).collect()
        })
    }

    /// 订阅事件旁路（GameProjector 已内建；sync/profile 随后 subscribe）
    pub(crate) fn publish(&self, event: &DomainEvent) {
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

    /// 提交作答：三级判题（规则 → Jev → 桶）→ 基线取样（insert 前，不含本次）→ 单事务落库 →
    /// 事件/游戏旁路结算 → SRS 旁路（建卡/提权/调度；失败 warn 不回滚，docs/03 §1 M4）
    pub async fn submit_answer(&self, attempt: &Attempt) -> Result<SubmitOutcome, EngineError> {
        let question = ambition_store::get_question(&self.pool, &attempt.qid)
            .await?
            .ok_or_else(|| EngineError::NotFound(attempt.qid.0.clone()))?;
        // 永不 Err：离线/灰区 → Pending（verdict=pending 同事务进 arbitration 桶）
        let grade = self.grader.grade(&question, &attempt.answer).await;
        // 假对基线取样必须在 insert 前（不含本次作答，docs/03 §1 / docs/08 §2）
        let guess = self
            .is_guess_attempt(attempt, Some(&attempt.attempt_id))
            .await;
        ambition_store::insert_attempt_tx(&self.pool, attempt, &grade).await?;
        let event = DomainEvent::AttemptGraded {
            attempt_id: attempt.attempt_id.clone(),
            qid: attempt.qid.clone(),
            verdict: grade.verdict,
            correct: matches!(grade.verdict, Verdict::Correct | Verdict::Acceptable),
        };
        let effects = self.settle(event).await;
        let srs = self.srs_after_attempt(attempt, &grade, guess).await;
        Ok(SubmitOutcome {
            grade,
            effects,
            srs,
        })
    }

    /// 旁路结算：事件广播 → 游戏效果取走 → 游戏状态落库（提交与人工裁决共用）
    pub(crate) async fn settle(&self, event: DomainEvent) -> Vec<ambition_gamify::GameEffect> {
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
    async fn test_start_practice_respects_limit() {
        let engine = Engine::open_memory().await.expect("打开");
        let all = engine.start_practice(0).await.expect("取题");
        let capped = engine.start_practice(1).await.expect("取题");
        assert_eq!(capped.len(), 1.min(all.len()), "limit 生效");
        assert_eq!(capped[0].qid, all[0].qid, "保持 qid 升序取前 N");
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
}
