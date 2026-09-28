//! 套卷模考（M5-W6，docs/06 §6 组卷 + docs/03 S3 mock_start/mock_finish）
//! 每题满分恒 1（docs/07 §1 各题型口径）→ 卷面分 = 已得分/总题数 × 卷面总分（未答 0 分、按 spec 分值换算）
use crate::*;

/// 模考会话（TUI 持有：题目按 Practice 同流作答、屏顶倒计时；末题 Next 触发 Cmd::MockFinish）
#[derive(Debug, Clone, PartialEq)]
pub struct MockSession {
    pub run_id: String,
    pub paper_id: String,
    pub title: String,
    pub deadline: i64,
    pub questions: Vec<Question>,
}

/// cet4 默认模考卷（MVP：全部规则可判——离线可判；34 分 / 30 分钟；AI 题型与真题结构随自定义 spec + 16-config 真题导入）
pub fn default_cet4_spec() -> PaperSpec {
    PaperSpec {
        track: "cet4".to_string(),
        title: "CET4 全真模考（MVP 卷）".to_string(),
        sections: vec![
            PaperSection {
                title: "Part I 词汇理解".to_string(),
                kinds: vec!["single_choice".to_string()],
                count: 10,
                points_each: 2.0,
            },
            PaperSection {
                title: "Part II 选词填空".to_string(),
                kinds: vec!["fill_blank".to_string()],
                count: 5,
                points_each: 2.0,
            },
            PaperSection {
                title: "Part III 多项选择".to_string(),
                kinds: vec!["multi_choice".to_string()],
                count: 2,
                points_each: 2.0,
            },
        ],
        minutes: 30,
    }
}

impl Engine {
    /// 组卷（quality ≥ IMPORT_QUALITY_FLOOR、按 section.kinds 过滤、qid 序稳定取用——同 spec 同卷幂等；
    /// 题量不足 → Paper 错误可见，不静默缩卷）
    pub async fn assemble_paper(&self, spec: &PaperSpec) -> Result<Paper, EngineError> {
        let all = ambition_store::list_questions(&self.pool).await?;
        let mut qids: Vec<String> = Vec::new();
        for section in &spec.sections {
            let mut taken = 0usize;
            for q in &all {
                if taken >= section.count as usize {
                    break;
                }
                if qids.contains(&q.qid.0) {
                    continue;
                }
                if q.quality_score < ambition_gen::IMPORT_QUALITY_FLOOR {
                    continue;
                }
                if !section.kinds.iter().any(|k| k == q.kind.kind_key()) {
                    continue;
                }
                qids.push(q.qid.0.clone());
                taken += 1;
            }
            if taken < section.count as usize {
                return Err(EngineError::Paper(format!(
                    "题目不足：{}（kind={:?}）需 {} 实得 {}（quality ≥ {}）",
                    section.title,
                    section.kinds,
                    section.count,
                    taken,
                    ambition_gen::IMPORT_QUALITY_FLOOR
                )));
            }
        }
        Ok(Paper {
            paper_id: format!(
                "paper-{}",
                ambition_core::canonical_json_hash(&serde_json::json!({ "spec": spec })).0
            ),
            spec: spec.clone(),
            qids,
        })
    }

    /// 开模考卷：默认卷 → 存卷（同 spec 同 paper_id 幂等 upsert）→ 起 run（deadline = now + minutes）
    /// → 返回会话（题目直供 TUI；run 行即估分数据源——predict_score 唯一在 docs/12、M6）
    pub async fn mock_start(&self) -> Result<MockSession, EngineError> {
        let spec = default_cet4_spec();
        let paper = self.assemble_paper(&spec).await?;
        ambition_store::save_paper(&self.pool, &paper).await?;
        let run_id = format!(
            "run-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let started = ambition_core::now_unix();
        let deadline = started + i64::from(spec.minutes) * 60;
        ambition_store::start_run(&self.pool, &run_id, &paper.paper_id, started, deadline).await?;
        let mut questions = Vec::with_capacity(paper.qids.len());
        for qid in &paper.qids {
            let q = ambition_store::get_question(&self.pool, &ambition_core::Qid(qid.clone()))
                .await?
                .ok_or_else(|| EngineError::NotFound(qid.clone()))?;
            questions.push(q);
        }
        Ok(MockSession {
            run_id,
            paper_id: paper.paper_id,
            title: spec.title.clone(),
            deadline,
            questions,
        })
    }

    /// 交卷：每 qid 只取最新一答求和（store::paper_score）→ 换算卷面分 → finish_run → PaperFinished
    pub async fn mock_finish(&self, run_id: &str) -> Result<(f32, f32), EngineError> {
        let run = ambition_store::load_run(&self.pool, run_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("run:{run_id}")))?;
        let paper = ambition_store::load_paper(&self.pool, &run.paper_id)
            .await?
            .ok_or_else(|| EngineError::NotFound(format!("paper:{}", run.paper_id)))?;
        let (sum_score, _sum_max) = ambition_store::paper_score(&self.pool, &paper.qids).await?;
        let total = paper.spec.total_points();
        let score = if paper.qids.is_empty() {
            0.0
        } else {
            (sum_score / paper.qids.len() as f32).clamp(0.0, 1.0) * total
        };
        ambition_store::finish_run(&self.pool, run_id, ambition_core::now_unix(), score, total)
            .await?;
        // 发布（docs/13：模考 Xp(10)；events 中毒取内层可见，P-8）
        ambition_core::lock_recover(&self.events, "Engine.events").push(
            DomainEvent::PaperFinished {
                run_id: run_id.to_string(),
                paper_id: run.paper_id.clone(),
                score,
                max: total,
            },
        );
        Ok((score, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{Answer, Attempt, AttemptMode, GradeDetail, GradedBy, Qid};

    fn correct_grade() -> GradeResult {
        GradeResult {
            verdict: Verdict::Correct,
            score: 1.0,
            max: 1.0,
            confidence: 1.0,
            graded_by: GradedBy::Rule,
            detail: GradeDetail::Plain,
            error_tags: vec![],
        }
    }

    #[test]
    fn test_default_cet4_spec_totals_points() {
        let spec = default_cet4_spec();
        assert_eq!(spec.total_points(), 34.0, "10×2 + 5×2 + 2×2");
        assert_eq!(spec.minutes, 30);
        assert_eq!(spec.sections.len(), 3);
    }

    #[tokio::test]
    async fn test_assemble_paper_picks_by_kind_and_is_idempotent() {
        let engine = Engine::open_memory().await.expect("内存库");
        let spec = default_cet4_spec();
        let paper = engine.assemble_paper(&spec).await.expect("组卷");
        assert_eq!(paper.qids.len(), 17, "17 题按 section 取满");
        // 按 kind 键过滤：翻译段8题（seed 内 translation 共8）
        let mut trans = spec.clone();
        trans.sections = vec![ambition_core::PaperSection {
            title: "翻译".to_string(),
            kinds: vec!["translation".to_string()],
            count: 8,
            points_each: 3.0,
        }];
        let tpaper = engine.assemble_paper(&trans).await.expect("翻译卷");
        assert_eq!(tpaper.qids.len(), 8);
        for qid in &tpaper.qids {
            let q = ambition_store::get_question(&engine.pool, &Qid(qid.clone()))
                .await
                .expect("取题")
                .expect("存在");
            assert_eq!(q.kind.kind_key(), "translation", "section.kind 键过滤");
        }
        // 同 spec 同 paper_id（幂等）
        let again = engine.assemble_paper(&spec).await.expect("再组卷");
        assert_eq!(again.paper_id, paper.paper_id, "同 spec 同卷");
        // 题量不足 → Paper 错误可见
        let mut huge = default_cet4_spec();
        huge.sections = vec![ambition_core::PaperSection {
            title: "超量".to_string(),
            kinds: vec!["single_choice".to_string()],
            count: 999,
            points_each: 1.0,
        }];
        let err = engine
            .assemble_paper(&huge)
            .await
            .expect_err("题量不足必须报错");
        assert!(matches!(err, EngineError::Paper(_)), "Paper 错误口径");
    }

    #[tokio::test]
    async fn test_mock_start_opens_run_with_deadline() {
        let engine = Engine::open_memory().await.expect("内存库");
        let session = engine.mock_start().await.expect("开卷");
        assert_eq!(session.questions.len(), 17, "默认卷 17 题");
        assert!(
            session.deadline > ambition_core::now_unix(),
            "deadline 在未来"
        );
        let run = ambition_store::load_run(&engine.pool, &session.run_id)
            .await
            .expect("查 run")
            .expect("存在");
        assert!(run.finished_at.is_none(), "开卷未收（估分数据未完成）");
        // 同 spec 再开：papers 幂等、runs 新增
        let _second = engine.mock_start().await.expect("再开卷");
        let (papers, runs): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM papers), (SELECT COUNT(*) FROM paper_runs)",
        )
        .fetch_one(&engine.pool)
        .await
        .expect("计数");
        assert_eq!((papers, runs), (1, 2), "papers 幂等 upsert、runs 每次新增");
    }

    #[tokio::test]
    async fn test_mock_finish_sums_latest_answer_and_publishes() {
        let engine = Engine::open_memory().await.expect("内存库");
        let session = engine.mock_start().await.expect("开卷");
        // 首题作答（tui 提交流的行等价：attempt + grade 事务）
        let attempt = Attempt {
            attempt_id: "tui-1".to_string(),
            qid: session.questions[0].qid.clone(),
            answer: Answer::Choice(vec![0]),
            timing: None,
            self_confidence: None,
            mode: AttemptMode::Practice,
        };
        ambition_store::insert_attempt_tx(&engine.pool, &attempt, &correct_grade())
            .await
            .expect("作答落库");
        let (score, max) = engine.mock_finish(&session.run_id).await.expect("交卷");
        assert_eq!(max, 34.0, "满分 = 卷面总分");
        assert!(
            (score - 34.0_f32 / 17.0).abs() < 1e-4,
            "答对1题 = 2.0 分（未答0分）实得 {score}"
        );
        let run = ambition_store::load_run(&engine.pool, &session.run_id)
            .await
            .expect("查")
            .expect("存在");
        assert!(run.finished_at.is_some(), "已收卷");
        assert_eq!(run.max, 34.0);
        let events = engine.drain_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::PaperFinished { .. })),
            "PaperFinished 已发布（docs/13 Xp(10)）"
        );
    }
}
