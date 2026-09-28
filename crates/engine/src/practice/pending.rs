//! 待裁定桶门面与成本面板：pending_bucket/resolve_pending/cost_summary（docs/03 §5，M3）

use super::*;

impl Engine {
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
        // M4：裁决补执行 SRS 处置（提交时 Pending 不动，docs/08 §1；旁路失败 warn 不回滚裁决）
        self.srs_settle_for(attempt_id, &grade).await;
        Ok(grade)
    }

    /// 裁决后补执行 SRS 处置（读回原作答 → 基线（排除本次）→ 假对 → 建卡/提权/调度）
    async fn srs_settle_for(&self, attempt_id: &str, grade: &GradeResult) {
        let attempt = match ambition_store::get_attempt(&self.pool, attempt_id).await {
            Ok(Some(attempt)) => attempt,
            Ok(None) => {
                tracing::warn!(%attempt_id, "srs settle skipped: attempt not found");
                return;
            }
            Err(err) => {
                tracing::warn!(error = %err, "srs settle skipped: attempt read failed");
                return;
            }
        };
        let guess = self.is_guess_attempt(&attempt, Some(attempt_id)).await;
        self.srs_after_attempt(&attempt, grade, guess).await;
    }

    /// 成本台账门面：区间 [from_unix, to_unix) 按 kind 聚合（llm_usage；docs/05 §1）
    pub async fn cost_summary(
        &self,
        from_unix: i64,
        to_unix: i64,
    ) -> Result<ambition_core::UsageSummary, EngineError> {
        Ok(ambition_store::usage_summary(&self.pool, from_unix, to_unix).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use ambition_core::{
        Attempt, AttemptMode, BlankItem, ChoiceBody, FillBody, OptionItem, QuestionKind,
        UsageRecord,
    };

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
    async fn test_srs_settle_for_runs_after_arbitration() {
        // 裁决补执行 SRS：reject → Wrong 建卡；accept → Correct 非假对无卡 = 不入卡（docs/08 §1）
        let engine = Engine::open_memory().await.expect("打开");
        let m = multi_pick_two();
        engine.add_question(&m).await.expect("注入");
        engine
            .submit_answer(&attempt_for(m.qid.clone(), vec![0], "sf-1"))
            .await
            .expect("提交");
        engine
            .submit_answer(&attempt_for(m.qid.clone(), vec![2], "sf-2"))
            .await
            .expect("提交");
        // accept（Correct 非假对无卡 → Skip 不入卡）
        engine.resolve_pending("sf-1", true).await.expect("裁决");
        let card_id = ambition_core::CardId::from_source(&ambition_core::CardSource::Question {
            qid: m.qid.clone(),
        });
        assert!(
            ambition_store::get_card(&engine.pool, &card_id.0)
                .await
                .expect("读卡")
                .is_none(),
            "accept 答对无卡不入卡"
        );
        // reject（Wrong → 建卡）
        engine.resolve_pending("sf-2", false).await.expect("裁决");
        let card = ambition_store::get_card(&engine.pool, &card_id.0)
            .await
            .expect("读卡")
            .expect("存在");
        assert_eq!(card.wrong_count, 1, "裁决判错建卡");
        // attempt 缺失 → 跳过不 panic（旁路降级）
        engine
            .srs_settle_for("ghost-attempt", &attempt_grade())
            .await;
    }

    /// srs_settle_for 测试用裁决后判分（Human Wrong）
    fn attempt_grade() -> GradeResult {
        GradeResult {
            verdict: Verdict::Wrong,
            score: 0.0,
            max: 1.0,
            confidence: 1.0,
            graded_by: ambition_core::GradedBy::Human,
            items: vec![],
            error_tags: vec![],
        }
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
        ambition_store::record_usage(
            &engine.pool,
            &UsageRecord::new("question_gen", "m1", 100, 20),
        )
        .await
        .expect("记账");
        ambition_store::record_usage(&engine.pool, &UsageRecord::new("jev", "jev-latest", 50, 10))
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
}
