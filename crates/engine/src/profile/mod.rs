//! 本地画像、个性化计划、周报与估分（docs/12 M6-W2/W3/W4）。

use crate::*;
use ambition_core::{
    DailyBudget, DailyPlan, ErrorTag, HeatMap, PlanItem, ProfileSnapshot, ScoreEstimate, Trend,
    WeakPoint, WeeklyReport,
};
use ambition_gen::TrackId;
use std::collections::{BTreeMap, BTreeSet};

const WEEK: i64 = 7 * 86_400;
const DECAY: f32 = 0.97;

impl Engine {
    /// 从 SQLite 作答与错因事实计算画像（离线、零 LLM）。
    pub async fn aggregate(&self, from: i64, to: i64) -> Result<ProfileSnapshot, EngineError> {
        if from >= to {
            return Err(EngineError::NotFound("画像时间范围无效".into()));
        }
        let rows = ambition_store::profile_attempt_rows(&self.pool, from, to).await?;
        let mut snapshot = ProfileSnapshot::empty(from, to);
        let mut weekly: BTreeMap<(String, String), [f32; 4]> = BTreeMap::new();
        for row in rows {
            snapshot.attempts += 1;
            if matches!(row.verdict.as_str(), "correct" | "acceptable") {
                snapshot.correct += 1;
            }
            if !matches!(row.verdict.as_str(), "wrong" | "partial") || row.max <= 0.0 {
                continue;
            }
            let weight = if row.verdict == "partial" { 0.5 } else { 1.0 };
            let age = ((to - row.created_at).max(0) as f32 / WEEK as f32).max(0.0);
            for raw in row.tags {
                let Some(tag) = ErrorTag::parse_tag(&raw) else {
                    continue;
                };
                if tag == ErrorTag::NoError {
                    continue;
                }
                let knowledge = if row.knowledge.is_empty() {
                    vec![row.qid.clone()]
                } else {
                    row.knowledge.clone()
                };
                for node in knowledge {
                    let key = (tag.as_str().to_string(), node);
                    snapshot
                        .heat
                        .cells
                        .entry(key.clone())
                        .and_modify(|v| *v += weight * DECAY.powf(age))
                        .or_insert(weight * DECAY.powf(age));
                    if row.created_at >= to - 4 * WEEK {
                        let week = ((to - row.created_at).max(0) / WEEK).min(3) as usize;
                        weekly.entry(key).or_default()[3 - week] += weight;
                    }
                }
            }
        }
        snapshot.weak_points = weak_points(&snapshot.heat, &weekly, 10);
        Ok(snapshot)
    }

    /// 热点趋势纯函数：最近四周密度斜率阈值 0.05。
    pub fn weak_points(&self, heat: &HeatMap, top: usize) -> Vec<WeakPoint> {
        weak_points(heat, &BTreeMap::new(), top)
    }

    /// 刷新近 26 周画像并保留可重算快照。
    pub async fn refresh(&self) -> Result<ProfileSnapshot, EngineError> {
        let to = ambition_core::now_unix();
        let from = to - 26 * WEEK;
        let snapshot = self.aggregate(from, to).await?;
        let payload =
            serde_json::to_string(&snapshot).map_err(|e| EngineError::Seed(e.to_string()))?;
        ambition_store::save_profile_snapshot(
            &self.pool,
            &ambition_store::ProfileSnapshotRow {
                id: ambition_core::NoteId::new_v7().0,
                from,
                to,
                payload,
                created_at: to,
            },
        )
        .await?;
        *ambition_core::lock_recover(&self.profile, "Engine.profile") = Some(snapshot.clone());
        Ok(snapshot)
    }

    /// 弱点驱动出题：薄弱点作为 GenSpec 标签，题型轮换覆盖常用题型。
    pub fn gen_spec_for(&self, budget: DailyBudget) -> ambition_gen::GenSpec {
        use ambition_gen::QuestionKindTag as K;
        let profile = ambition_core::lock_recover(&self.profile, "Engine.profile");
        let weak = profile
            .as_ref()
            .map(|p| {
                p.weak_points
                    .iter()
                    .take(5)
                    .filter_map(|w| ErrorTag::parse_tag(w.tag.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let count = u32::from(budget.drills);
        let mut spec = ambition_gen::GenSpec::new(
            "default",
            vec![K::SingleChoice, K::FillBlank, K::Translation],
            count,
        );
        spec.weak_points = weak;
        spec
    }

    /// 复仇战：错题卡中最近错过、且最近两次都正确的题目已出队。
    pub async fn vendetta_queue(&self) -> Result<Vec<ambition_core::Qid>, EngineError> {
        let wrong = ambition_store::wrong_cards(&self.pool).await?;
        let mut rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT a.qid, g.verdict FROM attempts a JOIN grades g ON g.attempt_id=a.attempt_id ORDER BY a.rowid DESC",
        ).fetch_all(&self.pool).await?;
        let mut cleared = BTreeSet::new();
        let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (qid, verdict) in rows.drain(..) {
            let outcomes = seen.entry(qid.clone()).or_default();
            if outcomes.len() < 3 {
                outcomes.push(verdict);
            }
            if outcomes.len() == 2 && outcomes.iter().all(|v| v == "correct" || v == "acceptable") {
                cleared.insert(qid);
            }
        }
        Ok(wrong
            .into_iter()
            .filter_map(|card| match card.source {
                ambition_core::CardSource::Question { qid } if !cleared.contains(&qid.0) => {
                    Some(qid)
                }
                _ => None,
            })
            .collect())
    }

    /// 每日交错计划（复习/新词/专项按预算展开，时长为零时不安排）。
    pub fn interleaved_plan(&self, daily: DailyBudget) -> DailyPlan {
        let mut counts = [daily.reviews, daily.new_words, daily.drills];
        let kinds = ["review", "new", "drill"];
        let mut items = Vec::new();
        while counts.iter().any(|n| *n > 0) {
            for i in 0..counts.len() {
                if counts[i] > 0 {
                    let target = counts[i].min(5);
                    counts[i] -= target;
                    items.push(PlanItem {
                        kind: kinds[i].into(),
                        title: format!("{} × {target}", kinds[i]),
                        target,
                        done: 0,
                    });
                }
            }
        }
        if daily.minutes == 0 {
            items.clear();
        }
        DailyPlan {
            date: today(),
            items,
        }
    }

    /// 周报：week 为该周周一 YYYY-MM-DD，区间左闭右开。
    pub async fn weekly_report(&self, week: &str) -> Result<WeeklyReport, EngineError> {
        let start = date_to_unix(week)
            .ok_or_else(|| EngineError::NotFound(format!("非法周起始日：{week}")))?;
        let end = start + WEEK;
        let snapshot = self.aggregate(start, end).await?;
        let rows = ambition_store::profile_attempt_rows(&self.pool, start, end).await?;
        let active: BTreeSet<String> = rows.iter().map(|r| unix_to_date(r.created_at)).collect();
        let runs = ambition_store::finished_runs(&self.pool, 200).await?;
        let recent: Vec<_> = runs.iter().take(5).map(|r| (r.score, r.max)).collect();
        let scores: Vec<f32> = runs
            .iter()
            .filter(|r| r.finished_at.is_some_and(|at| at >= start && at < end) && r.max > 0.0)
            .map(|r| r.score / r.max * 100.0)
            .collect();
        let avg = (!scores.is_empty()).then(|| scores.iter().sum::<f32>() / scores.len() as f32);
        let correct = snapshot.correct;
        Ok(WeeklyReport {
            week_start: week.to_string(),
            week_end: unix_to_date(end - 1),
            attempts: snapshot.attempts,
            correct,
            accuracy: if snapshot.attempts == 0 {
                0.0
            } else {
                correct as f32 / snapshot.attempts as f32
            },
            active_days: active.len().min(7) as u8,
            top_weak_points: snapshot.weak_points,
            mock_runs: scores.len() as u32,
            mock_average: avg,
            score_estimate: ambition_core::estimate_score(&recent, "default"),
        })
    }

    pub fn predict_score(
        &self,
        runs: &[ambition_store::PaperRun],
        target: &TrackId,
    ) -> ScoreEstimate {
        ambition_core::estimate_score(
            &runs.iter().map(|r| (r.score, r.max)).collect::<Vec<_>>(),
            &target.0,
        )
    }

    pub fn export_report_md(&self, report: &WeeklyReport) -> String {
        let mut out = format!(
            "# 每周学习报告 · {}\n\n- 作答：{} 次\n- 正确率：{:.1}%\n- 活跃：{} 天\n- 模考：{} 次\n",
            report.week_start,
            report.attempts,
            report.accuracy * 100.0,
            report.active_days,
            report.mock_runs
        );
        out.push_str("\n## 薄弱点\n");
        for point in &report.top_weak_points {
            out.push_str(&format!(
                "- {}（{}）：密度 {:.2}，趋势 {:?}\n",
                point.tag.label_zh(),
                point.knowledge,
                point.density,
                point.trend
            ));
        }
        out.push_str(&format!(
            "\n## 估分\n{:.1}（区间 {:.1}–{:.1}，样本 {}）\n",
            report.score_estimate.estimate,
            report.score_estimate.low,
            report.score_estimate.high,
            report.score_estimate.samples
        ));
        out
    }
}

fn weak_points(
    heat: &HeatMap,
    weekly: &BTreeMap<(String, String), [f32; 4]>,
    top: usize,
) -> Vec<WeakPoint> {
    let mut points: Vec<_> = heat
        .cells
        .iter()
        .filter_map(|((raw, knowledge), density)| {
            let tag = ErrorTag::parse_tag(raw)?;
            let seq = weekly
                .get(&(raw.clone(), knowledge.clone()))
                .copied()
                .unwrap_or([0.0; 4]);
            let slope = (seq[3] - seq[0]) / 3.0;
            let trend = if slope > 0.05 {
                Trend::Rising
            } else if slope < -0.05 {
                Trend::Falling
            } else {
                Trend::Stable
            };
            Some(WeakPoint {
                tag,
                knowledge: knowledge.clone(),
                density: *density,
                trend,
            })
        })
        .collect();
    points.sort_by(|a, b| b.density.total_cmp(&a.density));
    points.truncate(top);
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::{attempt_for, choice_question};
    use ambition_core::{GradeDetail, GradeResult, GradedBy, Verdict};

    #[tokio::test]
    async fn test_aggregate_partial_wrong_with_time_decay_by_knowledge_tag() {
        let engine = Engine::open_memory().await.expect("open");
        let q = choice_question();
        engine.add_question(&q).await.expect("题");
        for (id, verdict, score) in [
            ("pw-1", Verdict::Wrong, 0.0),
            ("pw-2", Verdict::Partial, 0.5),
        ] {
            let mut attempt = attempt_for(q.qid.clone(), vec![], id);
            attempt.attempt_id = id.into();
            let grade = GradeResult {
                verdict,
                score,
                max: 1.0,
                confidence: 1.0,
                graded_by: GradedBy::Rule,
                detail: GradeDetail::Plain,
                error_tags: vec![ErrorTag::Article],
            };
            ambition_store::insert_attempt_tx(&engine.pool, &attempt, &grade)
                .await
                .expect("存作答");
        }
        let now = ambition_core::now_unix();
        let snapshot = engine.aggregate(now - 60, now + 1).await.expect("画像");
        assert_eq!(snapshot.attempts, 2);
        assert!((snapshot.heat.cells[&("article".into(), q.qid.0.clone())] - 1.5).abs() < 0.01);
        assert_eq!(snapshot.weak_points[0].knowledge, q.qid.0);
    }

    #[tokio::test]
    async fn test_refresh_persists_profile_snapshot() {
        let engine = Engine::open_memory().await.expect("open");
        engine.refresh().await.expect("刷新");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM profile_snapshots")
            .fetch_one(&engine.pool)
            .await
            .expect("快照计数");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_gen_spec_for_uses_drill_budget() {
        let engine = Engine::open_memory().await.expect("open");
        let spec = engine.gen_spec_for(DailyBudget {
            minutes: 30,
            new_words: 5,
            reviews: 10,
            drills: 4,
        });
        assert_eq!(spec.count, 4);
        assert_eq!(spec.kinds.len(), 3);
        assert_eq!(
            engine
                .gen_spec_for(DailyBudget {
                    minutes: 0,
                    new_words: 0,
                    reviews: 0,
                    drills: 0,
                })
                .count,
            0,
            "零专项预算不额外出题"
        );
    }

    #[tokio::test]
    async fn test_interleaved_plan_chunks_each_task_and_zero_minutes_is_empty() {
        let engine = Engine::open_memory().await.expect("open");
        let plan = engine.interleaved_plan(DailyBudget {
            minutes: 30,
            new_words: 3,
            reviews: 6,
            drills: 2,
        });
        assert_eq!(plan.items.iter().map(|x| x.target).sum::<u16>(), 11);
        assert!(
            plan.items
                .iter()
                .map(|x| x.kind.as_str())
                .collect::<Vec<_>>()
                .windows(2)
                .all(|w| w[0] != w[1])
        );
        assert!(
            engine
                .interleaved_plan(DailyBudget {
                    minutes: 0,
                    new_words: 3,
                    reviews: 6,
                    drills: 2
                })
                .items
                .is_empty()
        );
    }

    #[tokio::test]
    async fn test_weekly_report_and_markdown_export_include_week() {
        let engine = Engine::open_memory().await.expect("open");
        let report = engine.weekly_report("2026-09-28").await.expect("周报");
        assert_eq!(report.week_start, "2026-09-28");
        assert!(engine.export_report_md(&report).contains("2026-09-28"));
    }

    #[tokio::test]
    async fn test_export_report_md_includes_score_estimate() {
        let engine = Engine::open_memory().await.expect("open");
        let report = engine.weekly_report("2026-09-28").await.unwrap();
        let markdown = engine.export_report_md(&report);
        assert!(markdown.contains("## 估分"));
    }

    #[tokio::test]
    async fn test_weak_points_sorts_by_density() {
        let engine = Engine::open_memory().await.expect("open");
        let mut heat = HeatMap::default();
        heat.add(ErrorTag::Article, "q1", 0.5);
        heat.add(ErrorTag::Article, "q2", 1.0);
        assert_eq!(engine.weak_points(&heat, 1)[0].knowledge, "q2");
    }

    #[tokio::test]
    async fn test_predict_score_returns_empty_sample_interval() {
        let engine = Engine::open_memory().await.expect("open");
        let estimate = engine.predict_score(&[], &TrackId("cet4".into()));
        assert_eq!(estimate.samples, 0);
    }

    #[tokio::test]
    async fn test_vendetta_queue_is_empty_without_wrong_cards() {
        let engine = Engine::open_memory().await.expect("open");
        assert!(engine.vendetta_queue().await.expect("复仇队列").is_empty());
    }
}
