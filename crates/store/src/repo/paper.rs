//! 套卷与模考 run（M5-W6，docs/02 §2）：papers/paper_runs 两行面 + paper_score 最新一答求和
use crate::*;

/// 模考 run 行（估分数据源——docs/12 `predict_score(&[PaperRun])` 唯一入口、M6 接入）
#[derive(Debug, Clone, PartialEq)]
pub struct PaperRun {
    pub run_id: String,
    pub paper_id: String,
    pub started_at: i64,
    pub deadline: i64,
    pub finished_at: Option<i64>,
    pub score: f32,
    pub max: f32,
}

/// 存卷（paper_id = spec 内容哈希——同 spec 同卷幂等 upsert）
pub async fn save_paper(
    pool: &SqlitePool,
    paper: &ambition_core::Paper,
) -> Result<(), sqlx::Error> {
    let payload =
        serde_json::to_string(paper).map_err(|err| sqlx::Error::Decode(err.to_string().into()))?;
    sqlx::query(
        "INSERT INTO papers (paper_id, track, title, payload, created_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(paper_id) DO UPDATE SET title = excluded.title, payload = excluded.payload",
    )
    .bind(&paper.paper_id)
    .bind(&paper.spec.track)
    .bind(&paper.spec.title)
    .bind(payload)
    .bind(ambition_core::now_unix())
    .execute(pool)
    .await?;
    Ok(())
}

/// 载卷（坏 payload 显式 Decode 上抛——不静默，P-4）
pub async fn load_paper(
    pool: &SqlitePool,
    paper_id: &str,
) -> Result<Option<ambition_core::Paper>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT payload FROM papers WHERE paper_id = ?")
        .bind(paper_id)
        .fetch_optional(pool)
        .await?;
    row.map(|(payload,)| {
        super::decode_payload::<ambition_core::Paper>(
            &payload,
            &format!("paper payload 损坏（{paper_id}）"),
        )
    })
    .transpose()
}

/// 开卷起 run（deadline = started + minutes；交卷前 finished_at = NULL）
pub async fn start_run(
    pool: &SqlitePool,
    run_id: &str,
    paper_id: &str,
    started_at: i64,
    deadline: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO paper_runs (run_id, paper_id, started_at, deadline) VALUES (?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind(paper_id)
    .bind(started_at)
    .bind(deadline)
    .execute(pool)
    .await?;
    Ok(())
}

/// 交卷收卷（分数以 spec 分值计——engine 换算后传入）
pub async fn finish_run(
    pool: &SqlitePool,
    run_id: &str,
    finished_at: i64,
    score: f32,
    max: f32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE paper_runs SET finished_at = ?, score = ?, max = ? WHERE run_id = ?")
        .bind(finished_at)
        .bind(score)
        .bind(max)
        .bind(run_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// paper_runs 行（run_id, paper_id, started, deadline, finished, score, max）
type PaperRunRow = (String, String, i64, i64, Option<i64>, f32, f32);

/// 载 run（估分/交卷前取 paper_id）
pub async fn load_run(pool: &SqlitePool, run_id: &str) -> Result<Option<PaperRun>, sqlx::Error> {
    let row: Option<PaperRunRow> = sqlx::query_as(
        "SELECT run_id, paper_id, started_at, deadline, finished_at, score, max FROM paper_runs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(run_id, paper_id, started_at, deadline, finished_at, score, max)| PaperRun {
            run_id,
            paper_id,
            started_at,
            deadline,
            finished_at,
            score,
            max,
        },
    ))
}

/// 卷内得分：每 qid 只取最新一答（attempts.rowid 序）求和——未作答题自然不计（max 均为1，docs/07 §1）
pub async fn paper_score(pool: &SqlitePool, qids: &[String]) -> Result<(f32, f32), sqlx::Error> {
    if qids.is_empty() {
        return Ok((0.0, 0.0));
    }
    let placeholders = vec!["?"; qids.len()].join(", ");
    let sql = format!(
        "SELECT COALESCE(SUM(score), 0) AS s, COALESCE(SUM(max), 0) AS m FROM (
             SELECT g.score AS score, g.max AS max,
                    ROW_NUMBER() OVER (PARTITION BY a.qid ORDER BY a.rowid DESC) AS rn
             FROM attempts a JOIN grades g ON g.attempt_id = a.attempt_id
             WHERE a.qid IN ({placeholders})
         ) latest WHERE rn = 1"
    );
    let mut query = sqlx::query_as::<_, (f32, f32)>(&sql);
    for qid in qids {
        query = query.bind(qid);
    }
    let (score, max) = query.fetch_one(pool).await?;
    Ok((score, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::{memory_pool, sample_attempt, sample_grade, sample_question};
    use ambition_core::{Qid, Verdict};

    fn paper_of(qids: &[&str]) -> ambition_core::Paper {
        ambition_core::Paper {
            paper_id: "p-mvp".to_string(),
            spec: ambition_core::PaperSpec {
                track: "cet4".to_string(),
                title: "测试卷".to_string(),
                sections: vec![],
                minutes: 30,
            },
            qids: qids.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn test_save_paper_upserts_by_id() {
        let pool = memory_pool().await;
        let paper = paper_of(&["qa"]);
        save_paper(&pool, &paper).await.expect("首存");
        let mut changed = paper.clone();
        changed.spec.title = "改题名".to_string();
        save_paper(&pool, &changed).await.expect("同卷覆盖");
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM papers")
            .fetch_one(&pool)
            .await
            .expect("计数");
        assert_eq!(n, 1, "同 paper_id 幂等 upsert");
    }

    #[tokio::test]
    async fn test_load_paper_roundtrips_and_misses_none() {
        let pool = memory_pool().await;
        let paper = paper_of(&["qa", "qb"]);
        save_paper(&pool, &paper).await.expect("存卷");
        let loaded = load_paper(&pool, "p-mvp")
            .await
            .expect("载卷")
            .expect("存在");
        assert_eq!(loaded, paper, "spec/qids 完整往返");
        assert!(
            load_paper(&pool, "nope").await.expect("查无").is_none(),
            "未知 id → None"
        );
    }

    #[tokio::test]
    async fn test_start_run_inserts_open_row() {
        let pool = memory_pool().await;
        save_paper(&pool, &paper_of(&["qa"])).await.expect("存卷");
        start_run(&pool, "run-1", "p-mvp", 100, 100 + 30 * 60)
            .await
            .expect("开卷");
        let run = load_run(&pool, "run-1")
            .await
            .expect("载 run")
            .expect("存在");
        assert_eq!(run.paper_id, "p-mvp");
        assert_eq!(run.deadline, 100 + 30 * 60);
        assert!(run.finished_at.is_none(), "开卷未收（估分数据未完成）");
    }

    #[tokio::test]
    async fn test_finish_run_persists_score_and_time() {
        let pool = memory_pool().await;
        save_paper(&pool, &paper_of(&["qa"])).await.expect("存卷");
        start_run(&pool, "run-1", "p-mvp", 100, 1900)
            .await
            .expect("开卷");
        finish_run(&pool, "run-1", 1200, 34.0, 40.0)
            .await
            .expect("收卷");
        let run = load_run(&pool, "run-1").await.expect("载").expect("存在");
        assert_eq!(run.finished_at, Some(1200));
        assert_eq!((run.score, run.max), (34.0, 40.0));
    }

    #[tokio::test]
    async fn test_load_run_returns_none_for_unknown() {
        let pool = memory_pool().await;
        assert!(
            load_run(&pool, "nope").await.expect("查").is_none(),
            "未知 run → None"
        );
    }

    #[tokio::test]
    async fn test_paper_score_sums_latest_per_qid() {
        let pool = memory_pool().await;
        let mut qa = sample_question();
        qa.qid = Qid("qa".to_string());
        let qb = {
            let mut q = sample_question();
            q.qid = Qid("qb".to_string());
            q
        };
        let qc = {
            let mut q = sample_question();
            q.qid = Qid("qc".to_string());
            q
        };
        crate::insert_question(&pool, &qa).await.expect("题a");
        crate::insert_question(&pool, &qb).await.expect("题b");
        crate::insert_question(&pool, &qc).await.expect("题c");
        // 一题两答：只取 rowid 最新一答（0.5 旧答案被 1.0 新答案取代，不叠加）
        let mut a1 = sample_attempt(Qid("qa".to_string()));
        a1.attempt_id = "t-1".to_string();
        let mut g1 = sample_grade(Verdict::Partial);
        g1.score = 0.5;
        crate::insert_attempt_tx(&pool, &a1, &g1)
            .await
            .expect("旧答");
        let mut a2 = sample_attempt(Qid("qa".to_string()));
        a2.attempt_id = "t-2".to_string();
        let g2 = sample_grade(Verdict::Correct);
        crate::insert_attempt_tx(&pool, &a2, &g2)
            .await
            .expect("新答");
        // 另一题一答；第三题未作答
        let mut b = sample_attempt(Qid("qb".to_string()));
        b.attempt_id = "t-3".to_string();
        let gb = sample_grade(Verdict::Correct);
        crate::insert_attempt_tx(&pool, &b, &gb)
            .await
            .expect("题b答");
        let (score, max) = paper_score(
            &pool,
            &["qa".to_string(), "qb".to_string(), "qc".to_string()],
        )
        .await
        .expect("计分");
        assert_eq!(
            (score, max),
            (2.0, 2.0),
            "最新一答 qa=1.0 + qb=1.0；qc 未答不计"
        );
    }
}
