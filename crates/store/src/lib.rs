//! Ambition store：业务数据持久化（SQLite）
//! 存储分层（docs/02-core-store）：SQLite 只存业务数据；词库/内置内容 = data/ 资源文件。
//! 迁移：schema_version 单行表 + MIGRATIONS 清单（只增不改，应用到最大版本）。

use ambition_core::{
    Attempt, GradeResult, Provenance, Qid, Question, QuestionContent, QuestionKind, UsageSummary,
    UsageTotal,
};
use serde_json::Value;
use sqlx::sqlite::SqlitePool;

/// v1：M1 骨架表（只增不改；新列一律进后续版本）
const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS questions (
    qid        TEXT PRIMARY KEY,
    payload    TEXT NOT NULL,
    difficulty INTEGER NOT NULL,
    lang       TEXT NOT NULL DEFAULT 'en',
    provenance TEXT NOT NULL DEFAULT 'seed',
    quality    REAL NOT NULL DEFAULT 1.0,
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
    payload    TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS error_tags (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    attempt_id TEXT NOT NULL,
    tag        TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS typing_sessions (
    session_id TEXT PRIMARY KEY,
    payload    TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS game_state (
    player    TEXT PRIMARY KEY,
    xp        INTEGER NOT NULL DEFAULT 0,
    streak    INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS llm_cache (
    cache_key  TEXT PRIMARY KEY,
    payload    TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS llm_usage (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT NOT NULL,
    model      TEXT NOT NULL,
    tokens_in  INTEGER NOT NULL,
    tokens_out INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// v2：M3 待裁定唯一桶（docs/07 §4；只增不改）
const MIGRATION_V2: &str = r#"
CREATE TABLE IF NOT EXISTS arbitration (
    attempt_id  TEXT PRIMARY KEY,
    qid         TEXT NOT NULL,
    answer      TEXT NOT NULL,
    reason      TEXT NOT NULL DEFAULT 'pipeline_undecided',
    status      TEXT NOT NULL DEFAULT 'pending',
    verdict     TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_arbitration_status ON arbitration(status);
"#;

/// 迁移清单（只增不改；序号 = 目标版本）
const MIGRATIONS: &[(i32, &str)] = &[(1, MIGRATION_V1), (2, MIGRATION_V2)];

/// 当前目标 schema 版本（清单最大号）
pub const SCHEMA_VERSION: i32 = 2;

/// 建库 + 迁移（幂等：已到目标版本则跳过；docs/02-core-store §2）
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")
        .execute(pool)
        .await?;
    let current: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version")
        .fetch_one(pool)
        .await?;
    for (version, sql) in MIGRATIONS {
        if *version <= current {
            continue;
        }
        let mut tx = pool.begin().await?;
        sqlx::query(sql).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(version)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    Ok(())
}

/// 写入题目（INSERT OR REPLACE 幂等：同 qid 覆盖；payload = 具名结构 QuestionContent）
pub async fn insert_question(pool: &SqlitePool, q: &Question) -> Result<(), sqlx::Error> {
    let payload = serde_json::to_string(&q.content()).unwrap_or_default();
    sqlx::query(
        "INSERT OR REPLACE INTO questions (qid, payload, difficulty, lang, provenance, quality)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&q.qid.0)
    .bind(payload)
    .bind(q.difficulty)
    .bind(&q.lang.0)
    .bind(q.provenance.as_str())
    .bind(q.quality_score)
    .execute(pool)
    .await?;
    Ok(())
}

/// 读取题目（None = 不存在；payload 解析失败返回 StoreError 语义 → sqlx RowNotFound 不妥，显式 Decode 错）
pub async fn get_question(pool: &SqlitePool, qid: &Qid) -> Result<Option<Question>, sqlx::Error> {
    let row = sqlx::query("SELECT payload, lang, provenance, quality FROM questions WHERE qid = ?")
        .bind(&qid.0)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let payload: String = sqlx::Row::get(&row, "payload");
    let content: QuestionContent = serde_json::from_str(&payload).map_err(|err| {
        sqlx::Error::Decode(format!("question payload 损坏（{qid:?}）: {err}").into())
    })?;
    let lang: String = sqlx::Row::get(&row, "lang");
    let provenance_raw: String = sqlx::Row::get(&row, "provenance");
    let quality: f32 = sqlx::Row::get(&row, "quality");
    let mut q = Question::with_content(content, parse_provenance(&provenance_raw));
    q.qid = qid.clone(); // 库中 qid 为事实源（防手改 payload 与重算不一致时行为可预期）
    q.lang = ambition_core::LangTag(lang);
    q.quality_score = quality;
    Ok(Some(q))
}

/// provenance 反序列化（未知值 → Seed，不 panic；与 as_str 同口径）
fn parse_provenance(raw: &str) -> Provenance {
    match raw {
        "generated" => Provenance::Generated,
        "imported" => Provenance::Imported,
        "collected" => Provenance::Collected,
        _ => Provenance::Seed,
    }
}

/// 列出题目（qid 升序，qpack 导出用）
pub async fn list_questions(pool: &SqlitePool) -> Result<Vec<Question>, sqlx::Error> {
    let rows = sqlx::query("SELECT qid FROM questions ORDER BY qid")
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let qid: String = sqlx::Row::get(&row, "qid");
        if let Some(q) = get_question(pool, &Qid(qid)).await? {
            out.push(q);
        }
    }
    Ok(out)
}

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
    let mut grade: GradeResult = serde_json::from_str(&payload).map_err(|err| {
        sqlx::Error::Decode(format!("grade payload 损坏（{attempt_id}）: {err}").into())
    })?;
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
    let mut content: QuestionContent = serde_json::from_str(&payload).map_err(|err| {
        sqlx::Error::Decode(format!("question payload 损坏（{qid:?}）: {err}").into())
    })?;
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

/// llm_cache TTL 淘汰：删除 created_at < before_unix 的行（返回删除数，docs/05 M3）
pub async fn cache_purge_before(pool: &SqlitePool, before_unix: i64) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM llm_cache WHERE created_at < datetime(?, 'unixepoch')")
        .bind(before_unix)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 打字会话落库（stats 为 JSON payload）
pub async fn insert_typing_session(
    pool: &SqlitePool,
    session_id: &str,
    stats: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR REPLACE INTO typing_sessions (session_id, payload) VALUES (?, ?)")
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

/// LLM 缓存读（None = 未命中；docs/05 llm_cache）
pub async fn cache_get(pool: &SqlitePool, cache_key: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query("SELECT payload FROM llm_cache WHERE cache_key = ?")
        .bind(cache_key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| sqlx::Row::get(&r, "payload")))
}

/// LLM 缓存写（同 key 覆盖）
pub async fn cache_put(
    pool: &SqlitePool,
    cache_key: &str,
    payload: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR REPLACE INTO llm_cache (cache_key, payload) VALUES (?, ?)")
        .bind(cache_key)
        .bind(payload)
        .execute(pool)
        .await?;
    Ok(())
}

/// LLM/Jev 用量记账（成本面板数据源，docs/05 llm_usage）
pub async fn record_usage(
    pool: &SqlitePool,
    kind: &str,
    model: &str,
    tokens_in: i64,
    tokens_out: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO llm_usage (kind, model, tokens_in, tokens_out) VALUES (?, ?, ?, ?)")
        .bind(kind)
        .bind(model)
        .bind(tokens_in)
        .bind(tokens_out)
        .execute(pool)
        .await?;
    Ok(())
}

/// 区间钳制到 SQLite datetime 可表达范围（[0, 9999-12-31]；i64::MAX 会令 datetime() 返回 NULL → 静默空结果）
fn clamp_unix_range(from_unix: i64, to_unix: i64) -> (i64, i64) {
    const SQLITE_MAX_UNIX: i64 = 253_402_300_799; // 9999-12-31 23:59:59
    (
        from_unix.clamp(0, SQLITE_MAX_UNIX),
        to_unix.clamp(0, SQLITE_MAX_UNIX),
    )
}

/// 用量汇总（区间内按 kind 聚合，[from_unix, to_unix) unix 秒；docs/05 usage_summary 数据源）
pub async fn usage_summary(
    pool: &SqlitePool,
    from_unix: i64,
    to_unix: i64,
) -> Result<UsageSummary, sqlx::Error> {
    let (from_unix, to_unix) = clamp_unix_range(from_unix, to_unix);
    let rows = sqlx::query(
        "SELECT kind, COALESCE(SUM(tokens_in),0) AS tin, COALESCE(SUM(tokens_out),0) AS tout
         FROM llm_usage
         WHERE created_at >= datetime(?, 'unixepoch') AND created_at < datetime(?, 'unixepoch')
         GROUP BY kind ORDER BY kind",
    )
    .bind(from_unix)
    .bind(to_unix)
    .fetch_all(pool)
    .await?;
    Ok(UsageSummary::from_rows(
        rows.iter()
            .map(|r| UsageTotal {
                kind: sqlx::Row::get(r, "kind"),
                tokens_in: sqlx::Row::get(r, "tin"),
                tokens_out: sqlx::Row::get(r, "tout"),
            })
            .collect(),
    ))
}

/// 用量汇总（不分区间，历史对照口径保留）
pub async fn usage_totals(pool: &SqlitePool) -> Result<Vec<(String, i64, i64)>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT kind, COALESCE(SUM(tokens_in),0) AS tin, COALESCE(SUM(tokens_out),0) AS tout
         FROM llm_usage GROUP BY kind ORDER BY kind",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                sqlx::Row::get(r, "kind"),
                sqlx::Row::get(r, "tin"),
                sqlx::Row::get(r, "tout"),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{
        Answer, Attempt, AttemptMode, BlankItem, GradeResult, GradedBy, Provenance, QuestionKind,
        Verdict,
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
    fn test_init_db_records_target_version_once() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                init_db(&pool).await.expect("二次迁移应幂等");
                init_db(&pool).await.expect("三次迁移仍幂等");
                let versions: Vec<i32> =
                    sqlx::query_scalar("SELECT version FROM schema_version ORDER BY version")
                        .fetch_all(&pool)
                        .await
                        .expect("版本表");
                assert_eq!(
                    versions,
                    (1..=SCHEMA_VERSION).collect::<Vec<_>>(),
                    "每个版本只记一次（v1 骨架 + v2 仲裁桶）"
                );
                let n: i64 = sqlx::query_scalar("SELECT count(*) FROM questions")
                    .fetch_one(&pool)
                    .await
                    .expect("查表");
                assert_eq!(n, 0);
            });
    }

    #[test]
    fn test_parse_provenance_maps_known_and_defaults_seed() {
        assert_eq!(parse_provenance("seed"), Provenance::Seed);
        assert_eq!(parse_provenance("generated"), Provenance::Generated);
        assert_eq!(parse_provenance("imported"), Provenance::Imported);
        assert_eq!(parse_provenance("collected"), Provenance::Collected);
        assert_eq!(
            parse_provenance("corrupted-value"),
            Provenance::Seed,
            "未知值回退 seed 不 panic"
        );
    }

    #[test]
    fn test_question_payload_is_named_struct() {
        let q = sample_question();
        let content = serde_json::to_string(&q.content()).expect("序列化");
        assert!(content.contains("\"stem\""), "具名结构字段: {content}");
        assert!(content.contains("\"kind\""), "具名结构字段: {content}");
        let parsed: QuestionContent = serde_json::from_str(&content).expect("可逆");
        assert_eq!(parsed, q.content());
    }

    #[test]
    fn test_insert_question_roundtrip_with_meta() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let mut q = sample_question();
                q.provenance = Provenance::Generated;
                q.quality_score = 0.85;
                insert_question(&pool, &q).await.expect("写入");
                insert_question(&pool, &q).await.expect("幂等覆盖");
                let got = get_question(&pool, &q.qid)
                    .await
                    .expect("读取")
                    .expect("存在");
                assert_eq!(got, q, "provenance/quality/lang 随题回读一致");
                assert_eq!(got.provenance, Provenance::Generated);
                assert!((got.quality_score - 0.85).abs() < f32::EPSILON);
                let missing = get_question(&pool, &Qid("nope".into()))
                    .await
                    .expect("读取");
                assert!(missing.is_none());
            });
    }

    #[test]
    fn test_list_questions_returns_sorted_bank() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                assert!(list_questions(&pool).await.expect("空库").is_empty());
                let a = sample_question();
                let mut b = Question::new(
                    "Second stem.".into(),
                    QuestionKind::FillBlank(ambition_core::FillBody {
                        sentence: "Second stem.".into(),
                        blanks: vec![BlankItem {
                            acceptable: vec!["b".into()],
                        }],
                    }),
                    3,
                    vec!["grammar".into()],
                    Provenance::Seed,
                );
                b.lang = ambition_core::LangTag("en".into());
                insert_question(&pool, &a).await.expect("写 a");
                insert_question(&pool, &b).await.expect("写 b");
                let listed = list_questions(&pool).await.expect("列表");
                assert_eq!(listed.len(), 2);
                let ids: Vec<&str> = listed.iter().map(|q| q.qid.0.as_str()).collect();
                let mut sorted = ids.clone();
                sorted.sort_unstable();
                assert_eq!(ids, sorted, "qid 升序（导出可复现）");
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
    fn test_insert_typing_session_persists_payload() {
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
                    "SELECT payload FROM typing_sessions WHERE session_id = 's-1'",
                )
                .fetch_one(&pool)
                .await
                .expect("读取");
                assert!(got.contains("42.5"));
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

    #[test]
    fn test_cache_get_returns_none_for_missing_key() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                assert_eq!(
                    cache_get(&pool, "never-written").await.expect("读缓存"),
                    None,
                    "未写先读 = None"
                );
            });
    }

    #[test]
    fn test_cache_put_persists_and_overrides_payload() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                cache_put(&pool, "k", "v1").await.expect("写入");
                cache_put(&pool, "k", "v2").await.expect("覆盖");
                assert_eq!(
                    cache_get(&pool, "k").await.expect("读回"),
                    Some("v2".to_string())
                );
            });
    }

    #[test]
    fn test_record_usage_inserts_row() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                record_usage(&pool, "question_gen", "m1", 11, 7)
                    .await
                    .expect("记账");
                let n: i64 = sqlx::query_scalar("SELECT count(*) FROM llm_usage")
                    .fetch_one(&pool)
                    .await
                    .expect("计数");
                assert_eq!(n, 1);
            });
    }

    #[test]
    fn test_usage_totals_groups_by_kind() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                assert!(usage_totals(&pool).await.expect("空汇总").is_empty());
                record_usage(&pool, "llm", "m1", 100, 20)
                    .await
                    .expect("记账1");
                record_usage(&pool, "jev", "jev-latest", 50, 10)
                    .await
                    .expect("记账2");
                record_usage(&pool, "llm", "m1", 30, 5)
                    .await
                    .expect("记账3");
                let totals = usage_totals(&pool).await.expect("汇总");
                assert_eq!(totals.len(), 2, "按 kind 聚合");
                assert!(totals.contains(&("llm".to_string(), 130, 25)));
                assert!(totals.contains(&("jev".to_string(), 50, 10)));
            });
    }

    #[test]
    fn test_cache_roundtrip_and_usage_totals() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                assert_eq!(
                    cache_get(&pool, "k1").await.expect("读缓存"),
                    None,
                    "未命中为 None"
                );
                cache_put(&pool, "k1", "cached-output")
                    .await
                    .expect("写缓存");
                cache_put(&pool, "k1", "cached-output-v2")
                    .await
                    .expect("同 key 覆盖");
                assert_eq!(
                    cache_get(&pool, "k1").await.expect("读缓存"),
                    Some("cached-output-v2".to_string())
                );
                record_usage(&pool, "llm", "test-model", 100, 20)
                    .await
                    .expect("记账1");
                record_usage(&pool, "jev", "jev-latest", 50, 10)
                    .await
                    .expect("记账2");
                record_usage(&pool, "llm", "test-model", 30, 5)
                    .await
                    .expect("记账3");
                let totals = usage_totals(&pool).await.expect("汇总");
                assert_eq!(totals.len(), 2, "按 kind 聚合");
                assert!(totals.contains(&("llm".to_string(), 130, 25)));
                assert!(totals.contains(&("jev".to_string(), 50, 10)));
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

    #[test]
    fn test_cache_purge_before_removes_expired_rows() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                cache_put(&pool, "fresh", "v1").await.expect("写缓存");
                sqlx::query(
                    "INSERT INTO llm_cache (cache_key, payload, created_at)
                     VALUES ('stale', 'old', datetime('now', '-30 days'))",
                )
                .execute(&pool)
                .await
                .expect("插旧行");
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let removed = cache_purge_before(&pool, now - 7 * 86_400)
                    .await
                    .expect("淘汰");
                assert_eq!(removed, 1, "只删过期行");
                assert_eq!(
                    cache_get(&pool, "fresh").await.expect("读缓存"),
                    Some("v1".to_string()),
                    "新缓存保留"
                );
                assert_eq!(cache_get(&pool, "stale").await.expect("读缓存"), None);
                let none_removed = cache_purge_before(&pool, 0).await.expect("淘汰");
                assert_eq!(none_removed, 0, "未来时间点无删除");
            });
    }

    #[test]
    fn test_clamp_unix_range_keeps_sqlite_datetime_expressible() {
        assert_eq!(
            clamp_unix_range(0, i64::MAX),
            (0, 253_402_300_799),
            "上界钳到 9999-12-31"
        );
        assert_eq!(clamp_unix_range(-5, 1_000), (0, 1_000), "下界钳到 0");
        assert_eq!(clamp_unix_range(10, 20), (10, 20), "范围内不变");
        assert_eq!(clamp_unix_range(i64::MIN, i64::MAX), (0, 253_402_300_799));
    }

    #[test]
    fn test_usage_summary_filters_range_and_groups_kind() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                record_usage(&pool, "question_gen", "m1", 100, 20)
                    .await
                    .expect("记账");
                record_usage(&pool, "jev", "jev-latest", 50, 10)
                    .await
                    .expect("记账");
                record_usage(&pool, "question_gen", "m1", 30, 5)
                    .await
                    .expect("记账");
                let summary = usage_summary(&pool, 0, i64::MAX).await.expect("汇总");
                assert_eq!(summary.rows.len(), 2, "按 kind 聚合");
                assert_eq!((summary.tokens_in, summary.tokens_out), (180, 35));
                let future = usage_summary(&pool, i64::MAX - 1, i64::MAX)
                    .await
                    .expect("汇总");
                assert_eq!(future.rows.len(), 0, "区间外为空");
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
