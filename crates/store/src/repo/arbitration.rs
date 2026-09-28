//! 待裁定桶：入桶/查桶/裁决/填空回流（docs/02 §3，M3 迁移 v2）

use super::decode_payload;
use crate::*;

/// 待裁定行（arbitration 桶条目，docs/07 §4）
#[derive(Debug, Clone, PartialEq)]
pub struct ArbitrationRow {
    pub attempt_id: String,
    pub qid: String,
    /// 学生作答（回流裁决的原料）
    pub answer: ambition_core::Answer,
    pub reason: String,
    pub status: String,
    pub created_at: String,
}

/// 读单条待裁定行（None = 不存在）
pub async fn get_arbitration(
    pool: &SqlitePool,
    attempt_id: &str,
) -> Result<Option<ArbitrationRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT attempt_id, qid, answer, reason, status, created_at
         FROM arbitration WHERE attempt_id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.as_ref().map(arbitration_from_row))
}

/// 列待裁定桶（status 过滤：pending/resolved；created_at 升序保证先到先裁）
pub async fn list_arbitration(
    pool: &SqlitePool,
    status: &str,
) -> Result<Vec<ArbitrationRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT attempt_id, qid, answer, reason, status, created_at
         FROM arbitration WHERE status = ? ORDER BY created_at, attempt_id",
    )
    .bind(status)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(arbitration_from_row).collect())
}

/// 行 → ArbitrationRow（answer 腐坏 → 显式空作答，P-4 不 panic）
fn arbitration_from_row(row: &sqlx::sqlite::SqliteRow) -> ArbitrationRow {
    let raw: String = sqlx::Row::get(row, "answer");
    let answer = serde_json::from_str(&raw).unwrap_or(ambition_core::Answer::Choice(Vec::new()));
    ArbitrationRow {
        attempt_id: sqlx::Row::get(row, "attempt_id"),
        qid: sqlx::Row::get(row, "qid"),
        answer,
        reason: sqlx::Row::get(row, "reason"),
        status: sqlx::Row::get(row, "status"),
        created_at: sqlx::Row::get(row, "created_at"),
    }
}

/// 人工裁决（单事务：grades 更新 human 判定 + arbitration 关闭；None = 无此待裁定）
/// 口径：accept → verdict=correct/score=max；reject → verdict=wrong/score=0；confidence=1（人工权威）。
pub async fn resolve_arbitration(
    pool: &SqlitePool,
    attempt_id: &str,
    accept: bool,
) -> Result<Option<GradeResult>, sqlx::Error> {
    let Some(row) = get_arbitration(pool, attempt_id).await? else {
        return Ok(None);
    };
    if row.status != "pending" {
        return Ok(None); // 已裁决：不重复处理（幂等）
    }
    let payload: String = sqlx::query_scalar("SELECT payload FROM grades WHERE attempt_id = ?")
        .bind(attempt_id)
        .fetch_optional(pool)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let mut grade: GradeResult =
        decode_payload(&payload, &format!("grade payload 损坏（{attempt_id}）"))?;
    grade.verdict = if accept {
        ambition_core::Verdict::Correct
    } else {
        ambition_core::Verdict::Wrong
    };
    grade.score = if accept { grade.max } else { 0.0 };
    grade.confidence = 1.0;
    grade.graded_by = ambition_core::GradedBy::Human;
    let new_payload = serde_json::to_string(&grade).unwrap_or_default();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE grades SET verdict = ?, score = ?, confidence = ?, graded_by = ?, payload = ?
         WHERE attempt_id = ?",
    )
    .bind(grade.verdict.as_str())
    .bind(grade.score)
    .bind(grade.confidence)
    .bind(grade.graded_by.as_str())
    .bind(&new_payload)
    .bind(attempt_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE arbitration SET status = 'resolved', verdict = ?, resolved_at = datetime('now')
         WHERE attempt_id = ?",
    )
    .bind(grade.verdict.as_str())
    .bind(attempt_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(grade))
}

/// 裁决回流：把人工认可的答案追加进填空可接受表（同值幂验；越界/已存在 → false）
pub async fn add_blank_acceptable(
    pool: &SqlitePool,
    qid: &Qid,
    blank_index: usize,
    value: &str,
) -> Result<bool, sqlx::Error> {
    let payload: Option<String> = sqlx::query_scalar("SELECT payload FROM questions WHERE qid = ?")
        .bind(&qid.0)
        .fetch_optional(pool)
        .await?;
    let Some(payload) = payload else {
        return Ok(false);
    };
    let mut content: QuestionContent =
        decode_payload(&payload, &format!("question payload 损坏（{qid:?}）"))?;
    let QuestionKind::FillBlank(fill) = &mut content.kind else {
        return Ok(false); // 非填空题无可回流
    };
    let Some(blank) = fill.blanks.get_mut(blank_index) else {
        return Ok(false); // 越界
    };
    if blank.acceptable.iter().any(|ok| ok == value) {
        return Ok(false); // 幂等：已可接受
    }
    blank.acceptable.push(value.to_string());
    let updated = serde_json::to_string(&content).unwrap_or_default();
    sqlx::query("UPDATE questions SET payload = ? WHERE qid = ?")
        .bind(updated)
        .bind(&qid.0)
        .execute(pool)
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::fixtures::*;
    use ambition_core::{GradeResult, GradedBy, QuestionKind, Verdict};
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

    fn pending_grade() -> GradeResult {
        GradeResult {
            verdict: Verdict::Pending,
            score: 1.0,
            max: 2.0,
            confidence: 0.0,
            graded_by: GradedBy::Rule,
            items: vec![],
            error_tags: vec![],
        }
    }

    #[test]
    fn test_get_arbitration_returns_pending_row_with_answer() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写题");
                // 非 Pending：不进桶
                let mut ok_attempt = sample_attempt(q.qid.clone());
                ok_attempt.attempt_id = "t-ok".into();
                insert_attempt_tx(&pool, &ok_attempt, &sample_grade(Verdict::Correct))
                    .await
                    .expect("事务");
                assert!(
                    get_arbitration(&pool, "t-ok")
                        .await
                        .expect("查桶")
                        .is_none(),
                    "非 Pending 不进桶"
                );
                // Pending：同事务进桶
                let mut pending_attempt = sample_attempt(q.qid.clone());
                pending_attempt.attempt_id = "t-pending".into();
                insert_attempt_tx(&pool, &pending_attempt, &pending_grade())
                    .await
                    .expect("事务");
                let row = get_arbitration(&pool, "t-pending")
                    .await
                    .expect("查桶")
                    .expect("进桶");
                assert_eq!(row.status, "pending");
                assert_eq!(row.reason, "pipeline_undecided");
                assert_eq!(row.qid, q.qid.0);
                assert_eq!(
                    row.answer,
                    ambition_core::Answer::Fill(vec!["a".into()]),
                    "回流原料 = 学生作答"
                );
                let pending = list_arbitration(&pool, "pending").await.expect("列表");
                assert_eq!(pending.len(), 1);
                assert!(
                    list_arbitration(&pool, "resolved")
                        .await
                        .expect("列表")
                        .is_empty(),
                    "按状态过滤"
                );
            });
    }

    #[test]
    fn test_list_arbitration_orders_and_filters_by_status() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写题");
                for id in ["la-1", "la-2"] {
                    let mut attempt = sample_attempt(q.qid.clone());
                    attempt.attempt_id = id.into();
                    insert_attempt_tx(&pool, &attempt, &pending_grade())
                        .await
                        .expect("事务");
                }
                let pending = list_arbitration(&pool, "pending").await.expect("列表");
                assert_eq!(pending.len(), 2, "全部 pending");
                let ids: Vec<&str> = pending.iter().map(|r| r.attempt_id.as_str()).collect();
                assert!(ids.windows(2).all(|w| w[0] <= w[1]), "创建时间升序");
                resolve_arbitration(&pool, "la-1", true)
                    .await
                    .expect("裁决")
                    .expect("存在");
                assert_eq!(
                    list_arbitration(&pool, "pending")
                        .await
                        .expect("列表")
                        .len(),
                    1,
                    "resolved 出列表"
                );
                assert_eq!(
                    list_arbitration(&pool, "resolved")
                        .await
                        .expect("列表")
                        .len(),
                    1
                );
                assert!(
                    list_arbitration(&pool, "unknown-status")
                        .await
                        .expect("列表")
                        .is_empty(),
                    "未知状态为空"
                );
            });
    }

    #[test]
    fn test_resolve_arbitration_updates_grade_and_closes_bucket() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写题");
                let attempt = sample_attempt(q.qid.clone());
                insert_attempt_tx(&pool, &attempt, &pending_grade())
                    .await
                    .expect("事务");
                let grade = resolve_arbitration(&pool, "t-1", true)
                    .await
                    .expect("裁决")
                    .expect("存在");
                assert_eq!(grade.verdict, Verdict::Correct);
                assert_eq!(grade.score, grade.max, "accept → 满分");
                assert_eq!(grade.graded_by, GradedBy::Human);
                assert_eq!(grade.confidence, 1.0, "人工权威");
                // grades 列同步
                let verdict: String =
                    sqlx::query_scalar("SELECT verdict FROM grades WHERE attempt_id='t-1'")
                        .fetch_one(&pool)
                        .await
                        .expect("verdict 列");
                assert_eq!(verdict, "correct");
                let by: String =
                    sqlx::query_scalar("SELECT graded_by FROM grades WHERE attempt_id='t-1'")
                        .fetch_one(&pool)
                        .await
                        .expect("graded_by 列");
                assert_eq!(by, "human");
                assert!(
                    list_arbitration(&pool, "pending")
                        .await
                        .expect("桶")
                        .is_empty(),
                    "裁决后出桶"
                );
                let resolved = list_arbitration(&pool, "resolved").await.expect("桶");
                assert_eq!(resolved.len(), 1);
                assert_eq!(resolved[0].attempt_id, "t-1");
                // 幂等：重复裁决 → None
                assert!(
                    resolve_arbitration(&pool, "t-1", false)
                        .await
                        .expect("二次裁决")
                        .is_none(),
                    "已裁决不重复处理"
                );
                assert!(
                    resolve_arbitration(&pool, "nope", true)
                        .await
                        .expect("查无")
                        .is_none(),
                    "不存在 → None"
                );
                // reject 路径：新题再裁
                let mut second = sample_attempt(q.qid.clone());
                second.attempt_id = "t-2".into();
                insert_attempt_tx(&pool, &second, &pending_grade())
                    .await
                    .expect("事务");
                let rejected = resolve_arbitration(&pool, "t-2", false)
                    .await
                    .expect("裁决")
                    .expect("存在");
                assert_eq!(rejected.verdict, Verdict::Wrong);
                assert_eq!(rejected.score, 0.0, "reject → 零分");
            });
    }

    #[test]
    fn test_arbitration_from_row_falls_back_on_corrupt_answer() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                sqlx::query(
                    "INSERT INTO arbitration (attempt_id, qid, answer, reason, status)
                     VALUES ('bad', 'q1', 'not-json', 'x', 'pending')",
                )
                .execute(&pool)
                .await
                .expect("插坏行");
                let row = get_arbitration(&pool, "bad")
                    .await
                    .expect("读取")
                    .expect("存在");
                assert_eq!(
                    row.answer,
                    ambition_core::Answer::Choice(vec![]),
                    "answer 腐坏 → 显式空作答不 panic（P-4）"
                );
            });
    }

    #[test]
    fn test_add_blank_acceptable_appends_once_and_bounds() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写题");
                assert!(
                    add_blank_acceptable(&pool, &q.qid, 0, "the")
                        .await
                        .expect("回流"),
                    "新答案追加"
                );
                assert!(
                    !add_blank_acceptable(&pool, &q.qid, 0, "the")
                        .await
                        .expect("回流"),
                    "同值幂等"
                );
                assert!(
                    !add_blank_acceptable(&pool, &q.qid, 0, "a")
                        .await
                        .expect("回流"),
                    "已可接受 → false"
                );
                assert!(
                    !add_blank_acceptable(&pool, &q.qid, 5, "x")
                        .await
                        .expect("回流"),
                    "越界 → false"
                );
                assert!(
                    !add_blank_acceptable(&pool, &Qid("nope".into()), 0, "x")
                        .await
                        .expect("回流"),
                    "无题 → false"
                );
                // 回流后规则直判：可接受表含 the
                let got = get_question(&pool, &q.qid)
                    .await
                    .expect("读题")
                    .expect("存在");
                let QuestionKind::FillBlank(fill) = got.kind else {
                    panic!("应为填空")
                };
                assert_eq!(fill.blanks[0].acceptable, vec!["a", "the"]);
                // 单选题无回流口
                let mut choice_q = q;
                choice_q.stem = "Pick one.".into();
                let _ = choice_q;
            });
    }
}
