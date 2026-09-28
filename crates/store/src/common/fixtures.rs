//! store 跨文件测试夹具（收口前 7 个文件逐字复制：memory_pool ×7、sample_* ×3；2026-09-27 复用合并）

use ambition_core::{
    Answer, Attempt, AttemptMode, BlankItem, FillBody, GradeDetail, GradeResult, GradedBy,
    Provenance, Qid, Question, QuestionKind, Verdict,
};

/// 填空样例题（题库读写与作答事务测试共用）
pub(crate) fn sample_question() -> Question {
    Question::new(
        "She is ___ student.".into(),
        QuestionKind::FillBlank(FillBody {
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

/// 样例作答（attempts/arbitration 单事务写入测试共用）
pub(crate) fn sample_attempt(qid: Qid) -> Attempt {
    Attempt {
        attempt_id: "t-1".into(),
        qid,
        answer: Answer::Fill(vec!["a".into()]),
        timing: None,
        self_confidence: None,
        mode: AttemptMode::Practice,
    }
}

/// 样例判分（裁决/落库测试共用）
pub(crate) fn sample_grade(verdict: Verdict) -> GradeResult {
    GradeResult {
        verdict,
        score: 1.0,
        max: 1.0,
        confidence: 1.0,
        graded_by: GradedBy::Rule,
        detail: GradeDetail::Plain,
        error_tags: vec![],
    }
}

/// 内存库夹具（sqlite::memory: + init_db 建表；全仓 7 个测试文件共用）
pub(crate) async fn memory_pool() -> sqlx::sqlite::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("内存库");
    crate::init_db(&pool).await.expect("建表");
    pool
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_question_fixture_is_fill_blank_article() {
        let q = sample_question();
        assert!(
            matches!(q.kind, QuestionKind::FillBlank(_)),
            "填空题夹具（题库/事务测试共用）"
        );
        assert_eq!(q.provenance, Provenance::Seed);
    }

    #[test]
    fn test_sample_attempt_fixture_is_practice_fill_answer() {
        let attempt = sample_attempt(Qid("q1".into()));
        assert_eq!(attempt.attempt_id, "t-1");
        assert!(matches!(attempt.answer, Answer::Fill(_)));
        assert_eq!(attempt.mode, AttemptMode::Practice);
    }

    #[test]
    fn test_sample_grade_fixture_carries_verdict_and_rule_path() {
        let grade = sample_grade(Verdict::Pending);
        assert_eq!(grade.verdict, Verdict::Pending, "verdict 透传");
        assert_eq!(grade.graded_by, GradedBy::Rule);
        assert_eq!((grade.score, grade.max), (1.0, 1.0));
    }

    #[tokio::test]
    async fn test_memory_pool_fixture_builds_schema_in_memory() {
        let pool = memory_pool().await;
        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&pool)
        .await
        .expect("查询表数");
        assert!(tables >= 5, "init_db 已建核心表（实际 {tables} 张）");
        pool.close().await;
    }
}
