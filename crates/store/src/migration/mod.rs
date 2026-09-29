//! schema 迁移与版本闸门：MIGRATION_V*/MIGRATIONS/SCHEMA_VERSION/init_db（docs/02 §2，只增不改）

use super::*;

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

/// v3：M4 复习卡队列（docs/02 §2 迁移 v3；单写者 = 08-adaptive 经 engine）
/// `due_at` 用整数 unix 秒（避开 `datetime()` NULL 静默坑，P-15）；`wrong_count` 为错词本查询列
const MIGRATION_V3: &str = r#"
CREATE TABLE IF NOT EXISTS srs_queue (
    card_id     TEXT PRIMARY KEY,
    source      TEXT NOT NULL,
    payload     TEXT NOT NULL,
    due_at      INTEGER NOT NULL,
    wrong_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_srs_queue_due ON srs_queue(due_at);
CREATE INDEX IF NOT EXISTS idx_srs_queue_wrong ON srs_queue(wrong_count);
"#;

const MIGRATION_V4: &str = r#"
CREATE TABLE IF NOT EXISTS templates (
    template_id TEXT PRIMARY KEY,
    track       TEXT NOT NULL,
    genre       TEXT NOT NULL,
    source      TEXT NOT NULL,
    payload     TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS essays (
    essay_id       TEXT PRIMARY KEY,
    qid            TEXT,
    prompt         TEXT NOT NULL,
    genre          TEXT NOT NULL,
    body           TEXT NOT NULL,
    word_count     INTEGER NOT NULL,
    scaffold_stage TEXT NOT NULL DEFAULT '',
    template_id    TEXT NOT NULL DEFAULT '',
    created_at     INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS essay_grades (
    essay_id  TEXT PRIMARY KEY,
    payload   TEXT NOT NULL,
    graded_at INTEGER NOT NULL
);
"#;

/// 迁移清单（只增不改；序号 = 目标版本）
const MIGRATIONS: &[(i32, &str)] = &[
    (1, MIGRATION_V1),
    (2, MIGRATION_V2),
    (3, MIGRATION_V3),
    (4, MIGRATION_V4),
    (5, MIGRATION_V5),
    (6, MIGRATION_V6),
    (7, MIGRATION_V7),
    (8, MIGRATION_V8),
    (9, MIGRATION_V9),
];

/// v5（M5-W6）：papers/paper_runs（组卷+模考 run——估分数据就绪；predict_score 唯一在 docs/12，M6，docs/02 §2）
const MIGRATION_V5: &str = r#"
CREATE TABLE IF NOT EXISTS papers (
    paper_id   TEXT PRIMARY KEY,
    track      TEXT NOT NULL,
    title      TEXT NOT NULL,
    payload    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS paper_runs (
    run_id      TEXT PRIMARY KEY,
    paper_id    TEXT NOT NULL,
    started_at  INTEGER NOT NULL,
    deadline    INTEGER NOT NULL,
    finished_at INTEGER,
    score       REAL NOT NULL DEFAULT 0,
    max         REAL NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_paper_runs_finished ON paper_runs (finished_at);
"#;

/// v6（M5-W7）：translations/tx_favorites（翻译工作台——tx_id 内容哈希与 llm_cache 同源，docs/02 §2）
const MIGRATION_V6: &str = r#"
CREATE TABLE IF NOT EXISTS translations (
    tx_id      TEXT PRIMARY KEY,
    dir        TEXT NOT NULL,
    source     TEXT NOT NULL,
    target     TEXT NOT NULL,
    payload    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tx_favorites (
    tx_id      TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_translations_source ON translations (source);
"#;

/// v7：笔记/日记/TODO（M6-W1，docs/02 §2）——FTS5 检索 + 双链反查 + 一日一篇 + 待办状态
const MIGRATION_V7: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
    note_id    TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,
    title      TEXT NOT NULL,
    body_md    TEXT NOT NULL,
    payload    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(title, body_md, tokenize = 'unicode61');
CREATE TABLE IF NOT EXISTS note_links (
    note_id    TEXT NOT NULL,
    ref_kind   TEXT NOT NULL,
    ref_value  TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (note_id, ref_kind, ref_value)
);
CREATE INDEX IF NOT EXISTS idx_note_links_ref ON note_links (ref_kind, ref_value);
CREATE TABLE IF NOT EXISTS diary_entries (
    entry_id TEXT PRIMARY KEY,
    date     TEXT NOT NULL UNIQUE,
    body     TEXT NOT NULL,
    review   TEXT
);
CREATE TABLE IF NOT EXISTS todos (
    todo_id    TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,
    title      TEXT NOT NULL,
    due        INTEGER,
    status     TEXT NOT NULL,
    meta       TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_todos_due ON todos (due);
"#;

/// v8：可重建的画像快照（事实仍来自 attempts/grades，docs/12 M6-W2）
const MIGRATION_V8: &str = r#"
CREATE TABLE IF NOT EXISTS profile_snapshots (
    snapshot_id TEXT PRIMARY KEY,
    from_at INTEGER NOT NULL,
    to_at INTEGER NOT NULL,
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_profile_snapshots_range ON profile_snapshots (from_at, to_at, created_at);
"#;

/// v9：游戏画像与事件记录（M6-W6；旧 game_state XP/连击表保留为兼容投影）
const MIGRATION_V9: &str = r#"
CREATE TABLE IF NOT EXISTS game_profile (
    player TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS game_events (
    event_id TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_game_events_created ON game_events (created_at);
"#;

/// 当前目标 schema 版本（清单最大号）
pub const SCHEMA_VERSION: i32 = 9;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;

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
}
