//! Ambition store：业务数据持久化（SQLite）
//! 存储分层（docs/02-core-store）：SQLite 只存业务数据；词库/内置内容 = data/ 资源文件。

use ambition_core::{Attempt, ChoiceBody, GradeResult, Qid, Question, QuestionKind};
use serde_json::Value;
use sqlx::sqlite::SqlitePool;

/// M1 业务表（词库表不在此 —— 只读资源见 docs/16-config-i18n §3.1）
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS questions (
    qid        TEXT PRIMARY KEY,
    content    TEXT NOT NULL,
    difficulty INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS attempts (
    attempt_id TEXT PRIMARY KEY,
    qid        TEXT NOT NULL,
    answer     TEXT NOT NULL,
    mode       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS grades (
    attempt_id TEXT PRIMARY KEY,
    verdict    TEXT NOT NULL,
    score      REAL NOT NULL,
    max        REAL NOT NULL,
    confidence REAL NOT NULL,
    graded_by  TEXT NOT NULL,
    items      TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS error_tags (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    attempt_id TEXT NOT NULL,
    tag        TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS typing_sessions (
    session_id TEXT PRIMARY KEY,
    stats      TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS game_state (
    player    TEXT PRIMARY KEY,
    xp        INTEGER NOT NULL DEFAULT 0,
    streak    INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// 建库（幂等；迁移版本化见 docs/02-core-store §2）
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(SCHEMA).execute(pool).await?;
    Ok(())
}

/// 题目内容 JSON：(stem, kind, difficulty, tags) —— qid 单列
fn question_content(q: &Question) -> String {
    serde_json::to_string(&(q.stem.clone(), &q.kind, q.difficulty, q.tags.clone()))
        .unwrap_or_default()
}

/// 写入题目（INSERT OR REPLACE 幂等：同 qid 覆盖）
pub async fn insert_question(pool: &SqlitePool, q: &Question) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR REPLACE INTO questions (qid, content, difficulty) VALUES (?, ?, ?)")
        .bind(&q.qid.0)
        .bind(question_content(q))
        .bind(q.difficulty)
        .execute(pool)
        .await?;
    Ok(())
}

/// 读取题目（None = 不存在）
pub async fn get_question(pool: &SqlitePool, qid: &Qid) -> Result<Option<Question>, sqlx::Error> {
    let row = sqlx::query("SELECT content FROM questions WHERE qid = ?")
        .bind(&qid.0)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let content: String = sqlx::Row::get(&row, "content");
    let parsed: (String, QuestionKind, u8, Vec<String>) =
        serde_json::from_str(&content).unwrap_or((
            String::new(),
            QuestionKind::SingleChoice(ChoiceBody {
                options: vec![],
                answer: vec![],
            }),
            1,
            Vec::new(),
        ));
    Ok(Some(Question {
        qid: qid.clone(),
        kind: parsed.1,
        stem: parsed.0,
        difficulty: parsed.2,
        tags: parsed.3,
    }))
}

/// 提交作答 = 单事务（attempts + grades + error_tags，docs/02-core-store §事务）
pub async fn insert_attempt_tx(
    pool: &SqlitePool,
    attempt: &Attempt,
    grade: &GradeResult,
) -> Result<(), sqlx::Error> {
    let answer = serde_json::to_string(attempt).unwrap_or_default();
    let items = serde_json::to_string(grade).unwrap_or_default();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO attempts (attempt_id, qid, answer, mode) VALUES (?, ?, ?, ?)")
        .bind(&attempt.attempt_id)
        .bind(&attempt.qid.0)
        .bind(answer)
        .bind(format!("{:?}", attempt.mode))
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO grades (attempt_id, verdict, score, max, confidence, graded_by, items)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&attempt.attempt_id)
    .bind(format!("{:?}", grade.verdict))
    .bind(grade.score)
    .bind(grade.max)
    .bind(grade.confidence)
    .bind(format!("{:?}", grade.graded_by))
    .bind(items)
    .execute(&mut *tx)
    .await?;
    for tag in &grade.error_tags {
        sqlx::query("INSERT INTO error_tags (attempt_id, tag) VALUES (?, ?)")
            .bind(&attempt.attempt_id)
            .bind(tag.as_str())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 打字会话落库（stats 为 TypingStats 的 JSON）
pub async fn insert_typing_session(
    pool: &SqlitePool,
    session_id: &str,
    stats: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR REPLACE INTO typing_sessions (session_id, stats) VALUES (?, ?)")
        .bind(session_id)
        .bind(stats.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// 读取游戏状态（不存在返回默认零值）
pub async fn load_game_state(pool: &SqlitePool, player: &str) -> Result<(i64, i64), sqlx::Error> {
    let row = sqlx::query("SELECT xp, streak FROM game_state WHERE player = ?")
        .bind(player)
        .fetch_optional(pool)
        .await?;
    Ok(row.map_or((0, 0), |row| {
        (sqlx::Row::get(&row, "xp"), sqlx::Row::get(&row, "streak"))
    }))
}

/// 保存游戏状态
pub async fn save_game_state(
    pool: &SqlitePool,
    player: &str,
    xp: i64,
    streak: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO game_state (player, xp, streak) VALUES (?, ?, ?)
         ON CONFLICT(player) DO UPDATE SET xp = excluded.xp, streak = excluded.streak,
         updated_at = datetime('now')",
    )
    .bind(player)
    .bind(xp)
    .bind(streak)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{
        Answer, Attempt, AttemptMode, BlankItem, GradeResult, GradedBy, QuestionKind, Verdict,
    };
    use serde_json::json;
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
    fn test_init_db_is_idempotent() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                init_db(&pool).await.expect("二次建表应幂等");
                let row = sqlx::query("SELECT count(*) AS n FROM questions")
                    .fetch_one(&pool)
                    .await
                    .expect("查表");
                let n: i64 = sqlx::Row::get(&row, "n");
                assert_eq!(n, 0);
            });
    }

    #[test]
    fn test_question_content_serializes_tuple() {
        let q = sample_question();
        let content = question_content(&q);
        let parsed: (String, QuestionKind, u8, Vec<String>) =
            serde_json::from_str(&content).expect("内容可逆");
        assert_eq!(parsed.0, q.stem);
        assert_eq!(parsed.2, q.difficulty);
        assert_eq!(parsed.3, q.tags);
    }

    #[test]
    fn test_insert_question_roundtrip() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写入");
                insert_question(&pool, &q).await.expect("幂等覆盖");
                let got = get_question(&pool, &q.qid)
                    .await
                    .expect("读取")
                    .expect("存在");
                assert_eq!(got, q);
                let missing = get_question(&pool, &Qid("nope".into()))
                    .await
                    .expect("读取");
                assert!(missing.is_none());
            });
    }

    #[test]
    fn test_get_question_returns_some_and_none() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                assert!(
                    get_question(&pool, &Qid("none".into()))
                        .await
                        .expect("读取")
                        .is_none()
                );
                let q = sample_question();
                insert_question(&pool, &q).await.expect("写入");
                let got = get_question(&pool, &q.qid)
                    .await
                    .expect("读取")
                    .expect("存在");
                assert_eq!(got.qid, q.qid);
                assert_eq!(got.kind, q.kind);
            });
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
    fn test_insert_typing_session_persists_stats() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let stats = json!({ "wpm": 42.5, "accuracy": 98.0, "correct": 100, "total": 102 });
                insert_typing_session(&pool, "s-1", &stats)
                    .await
                    .expect("写入");
                let got: String = sqlx::query_scalar(
                    "SELECT stats FROM typing_sessions WHERE session_id = 's-1'",
                )
                .fetch_one(&pool)
                .await
                .expect("读取");
                assert!(got.contains("42.5"));
            });
    }

    #[test]
    fn test_load_game_state_returns_saved() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                assert_eq!(load_game_state(&pool, "hero").await.expect("默认"), (0, 0));
                save_game_state(&pool, "hero", 42, 3).await.expect("保存");
                assert_eq!(load_game_state(&pool, "hero").await.expect("读取"), (42, 3));
            });
    }

    #[test]
    fn test_save_game_state_roundtrip() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                assert_eq!(load_game_state(&pool, "hero").await.expect("默认"), (0, 0));
                save_game_state(&pool, "hero", 120, 7).await.expect("保存");
                save_game_state(&pool, "hero", 260, 8).await.expect("更新");
                assert_eq!(
                    load_game_state(&pool, "hero").await.expect("读取"),
                    (260, 8)
                );
            });
    }
}
