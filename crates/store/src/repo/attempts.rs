//! 作答落库单事务：attempts+grades+error_tags 原子写入（docs/02 §3）

use super::decode_payload;
use crate::*;

/// 提交作答 = 单事务（attempts + grades + error_tags + arbitration，docs/02-core-store §事务）
pub async fn insert_attempt_tx(
    pool: &SqlitePool,
    attempt: &Attempt,
    grade: &GradeResult,
) -> Result<(), sqlx::Error> {
    let answer = serde_json::to_string(attempt).unwrap_or_default();
    let payload = serde_json::to_string(grade).unwrap_or_default();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO attempts (attempt_id, qid, answer, mode) VALUES (?, ?, ?, ?)")
        .bind(&attempt.attempt_id)
        .bind(&attempt.qid.0)
        .bind(answer)
        .bind(attempt.mode.as_str())
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO grades (attempt_id, verdict, score, max, confidence, graded_by, payload)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&attempt.attempt_id)
    .bind(grade.verdict.as_str())
    .bind(grade.score)
    .bind(grade.max)
    .bind(grade.confidence)
    .bind(grade.graded_by.as_str())
    .bind(payload)
    .execute(&mut *tx)
    .await?;
    for tag in &grade.error_tags {
        sqlx::query("INSERT INTO error_tags (attempt_id, tag) VALUES (?, ?)")
            .bind(&attempt.attempt_id)
            .bind(tag.as_str())
            .execute(&mut *tx)
            .await?;
    }
    // Pending = 流水线未定夺 → 同事务进待裁定唯一桶（唯一写入方：本事务）
    if grade.verdict == ambition_core::Verdict::Pending {
        let answer_json =
            serde_json::to_string(&attempt.answer).unwrap_or_else(|_| "null".to_string());
        sqlx::query(
            "INSERT OR REPLACE INTO arbitration (attempt_id, qid, answer, reason, status)
             VALUES (?, ?, ?, 'pipeline_undecided', 'pending')",
        )
        .bind(&attempt.attempt_id)
        .bind(&attempt.qid.0)
        .bind(answer_json)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 读回整条 Attempt（attempts.answer 列存完整 Attempt JSON；docs/02 §2 迁移 v3）
pub async fn get_attempt(
    pool: &SqlitePool,
    attempt_id: &str,
) -> Result<Option<Attempt>, sqlx::Error> {
    let answer: Option<String> =
        sqlx::query_scalar("SELECT answer FROM attempts WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_optional(pool)
            .await?;
    let Some(payload) = answer else {
        return Ok(None);
    };
    let attempt: Attempt = decode_payload(&payload, &format!("attempt 载荷损坏（{attempt_id}）"))?;
    Ok(Some(attempt))
}

/// 近 N 次作答计时（假对基线原料：total_ms 滚动样本，docs/08 §2；按创建时间倒序取 N）
/// SQL 层过滤无计时行（同秒写入时 LIMIT 不会命中 timing=null 的行）；
/// `exclude` = 排除的 attempt_id（基线不含本次作答——docs/03 §1「insert 前取样」的裁决路径等价表达）
pub async fn recent_timings(
    pool: &SqlitePool,
    limit: i64,
    exclude: Option<&str>,
) -> Result<Vec<ambition_core::Timing>, sqlx::Error> {
    // attempts.answer 是完整 Attempt JSON（含 timing）；json_extract 过滤 null
    let mut sql = String::from(
        "SELECT answer FROM attempts
         WHERE json_extract(answer, '$.timing') IS NOT NULL",
    );
    if exclude.is_some() {
        sql.push_str(" AND attempt_id != ?");
    }
    sql.push_str(" ORDER BY created_at DESC, attempt_id DESC LIMIT ?");
    let mut query = sqlx::query_scalar(&sql);
    if let Some(id) = exclude {
        query = query.bind(id);
    }
    let payloads: Vec<String> = query.bind(limit).fetch_all(pool).await?;
    let mut timings = Vec::with_capacity(payloads.len());
    for payload in payloads {
        // 坏行跳过（只取样本用途；get_attempt 才显式上抛坏载荷）
        if let Ok(attempt) = serde_json::from_str::<Attempt>(&payload)
            && let Some(t) = attempt.timing
        {
            timings.push(t);
        }
    }
    Ok(timings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use ambition_core::{AttemptMode, GradedBy, Verdict};

    #[test]
    fn test_insert_attempt_tx_writes_all_or_nothing() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写题");
                let attempt = sample_attempt(q.qid.clone());
                let mut grade = sample_grade(Verdict::Correct);
                grade.error_tags = vec![ambition_core::ErrorTag::Article];
                insert_attempt_tx(&pool, &attempt, &grade)
                    .await
                    .expect("事务");
                let grades: i64 = sqlx::query_scalar("SELECT count(*) FROM grades")
                    .fetch_one(&pool)
                    .await
                    .expect("grades 计数");
                let tags: i64 = sqlx::query_scalar("SELECT count(*) FROM error_tags")
                    .fetch_one(&pool)
                    .await
                    .expect("error_tags 计数");
                assert_eq!(grades, 1);
                assert_eq!(tags, 1);
            });
    }

    #[test]
    fn test_get_attempt_reads_back_full_attempt_or_none() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写题");
                let mut attempt = sample_attempt(q.qid.clone());
                attempt.attempt_id = "ga-1".into();
                attempt.timing = Some(ambition_core::Timing {
                    total_ms: 4_200,
                    revision_count: 1,
                });
                let grade = sample_grade(Verdict::Correct);
                insert_attempt_tx(&pool, &attempt, &grade)
                    .await
                    .expect("事务");
                let got = get_attempt(&pool, "ga-1")
                    .await
                    .expect("读回")
                    .expect("存在");
                assert_eq!(got, attempt, "整条 Attempt 往返无损");
                assert_eq!(got.timing.expect("计时").total_ms, 4_200);
                assert!(
                    get_attempt(&pool, "nope").await.expect("查询").is_none(),
                    "不存在 → None"
                );
            });
    }

    #[test]
    fn test_recent_timings_returns_latest_samples_with_timing() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写题");
                // 三条作答：两条带计时、一条无计时（timing=None 不进样本）
                for (id, ms) in [
                    ("rt-1", Some(1_000u64)),
                    ("rt-2", Some(3_000)),
                    ("rt-3", None),
                ] {
                    let mut attempt = sample_attempt(q.qid.clone());
                    attempt.attempt_id = id.into();
                    attempt.timing = ms.map(|total_ms| ambition_core::Timing {
                        total_ms,
                        revision_count: 0,
                    });
                    let grade = sample_grade(Verdict::Correct);
                    insert_attempt_tx(&pool, &attempt, &grade)
                        .await
                        .expect("事务");
                }
                let timings = recent_timings(&pool, 10, None).await.expect("取样本");
                assert_eq!(timings.len(), 2, "无计时作答不进样本");
                assert!(timings.iter().any(|t| t.total_ms == 1_000));
                let capped = recent_timings(&pool, 1, None).await.expect("限量");
                assert_eq!(capped.len(), 1, "limit 生效");
                assert!(recent_timings(&pool, 0, None).await.expect("零").is_empty());
                // exclude：基线不含指定作答（docs/03 §1 insert 前取样等价）
                let excluded = recent_timings(&pool, 10, Some("rt-2"))
                    .await
                    .expect("排除")
                    .into_iter()
                    .map(|t| t.total_ms)
                    .collect::<Vec<_>>();
                assert!(!excluded.contains(&3_000), "rt-2 被排除");
                assert_eq!(excluded.len(), 1, "剩 rt-1");
            });
    }

    #[test]
    fn test_attempt_and_grade_persist_snake_case_enums() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写题");
                let mut attempt = sample_attempt(q.qid.clone());
                attempt.mode = AttemptMode::Review;
                let mut grade = sample_grade(Verdict::Pending);
                grade.graded_by = GradedBy::LlmReview;
                insert_attempt_tx(&pool, &attempt, &grade)
                    .await
                    .expect("事务");
                let mode: String =
                    sqlx::query_scalar("SELECT mode FROM attempts WHERE attempt_id='t-1'")
                        .fetch_one(&pool)
                        .await
                        .expect("mode 列");
                assert_eq!(mode, "review", "枚举列 snake_case 而非 Debug 字符串");
                let verdict: String =
                    sqlx::query_scalar("SELECT verdict FROM grades WHERE attempt_id='t-1'")
                        .fetch_one(&pool)
                        .await
                        .expect("verdict 列");
                assert_eq!(verdict, "pending");
                let by: String =
                    sqlx::query_scalar("SELECT graded_by FROM grades WHERE attempt_id='t-1'")
                        .fetch_one(&pool)
                        .await
                        .expect("graded_by 列");
                assert_eq!(by, "llm_review");
                let payload: String =
                    sqlx::query_scalar("SELECT payload FROM grades WHERE attempt_id='t-1'")
                        .fetch_one(&pool)
                        .await
                        .expect("payload 列");
                assert!(
                    payload.contains("\"detail\""),
                    "负载列名 payload（M5 detail 字段随行）: {payload}"
                );
            });
    }
}
