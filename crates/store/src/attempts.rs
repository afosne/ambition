//! 作答落库单事务：attempts+grades+error_tags 原子写入（docs/02 §3）

use super::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{
        Answer, Attempt, AttemptMode, BlankItem, GradeResult, GradedBy, Provenance, QuestionKind,
        Verdict,
    };
    use sqlx::sqlite::SqlitePoolOptions;

    async fn memory_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("内存库");
        init_db(&pool).await.expect("建表");
        pool
    }

    fn sample_question() -> Question {
        Question::new(
            "She is ___ student.".into(),
            QuestionKind::FillBlank(ambition_core::FillBody {
                sentence: "She is ___ student.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["a".into()],
                }],
            }),
            2,
            vec!["article".into()],
            Provenance::Seed,
        )
    }

    fn sample_attempt(qid: Qid) -> Attempt {
        Attempt {
            attempt_id: "t-1".into(),
            qid,
            answer: Answer::Fill(vec!["a".into()]),
            timing: None,
            self_confidence: None,
            mode: AttemptMode::Practice,
        }
    }

    fn sample_grade(verdict: Verdict) -> GradeResult {
        GradeResult {
            verdict,
            score: 1.0,
            max: 1.0,
            confidence: 1.0,
            graded_by: GradedBy::Rule,
            items: vec![],
            error_tags: vec![],
        }
    }

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
                assert!(payload.contains("\"items\""), "负载列名 payload: {payload}");
            });
    }
}
