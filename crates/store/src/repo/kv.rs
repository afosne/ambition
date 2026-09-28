//! 键值面：llm_cache（TTL 淘汰）/typing_sessions/game_state 持久化（docs/02 §4）

use super::usage::clamp_unix_range;
use crate::*;

/// llm_cache TTL 淘汰：删除 created_at < before_unix 的行（返回删除数，docs/05 M3）
pub async fn cache_purge_before(pool: &SqlitePool, before_unix: i64) -> Result<u64, sqlx::Error> {
    // P-15 防御：超界上界会让 datetime() 返回 NULL → 静默不删；与 usage_summary 同口径钉到可表达区间
    let before_unix = clamp_unix_range(0, before_unix).1;
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

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_cache_purge_before_clamps_out_of_range_for_sqlite_datetime() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                cache_put(&pool, "old", "v").await.expect("写缓存");
                // P-15：i64::MAX 超出 datetime() 表达范围——未钉住会返回 NULL → 静默不删
                let removed = cache_purge_before(&pool, i64::MAX).await.expect("淘汰");
                assert_eq!(removed, 1, "超界上界钳到 9999-12-31 → 全部落入淘汰范围");
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
