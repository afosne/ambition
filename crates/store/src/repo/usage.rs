//! 成本台账仓储：record_usage/usage_summary/usage_totals + datetime 钳制（P-15，docs/02 §5）

use crate::*;

/// LLM/Jev 用量记账（成本面板数据源，docs/05 llm_usage；载荷单结构 UsageRecord——
/// 2026-09-27 跨模块收口：原 4 位置参数改单对象，调用方只传 1 参）
pub async fn record_usage(pool: &SqlitePool, record: &UsageRecord) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO llm_usage (kind, model, tokens_in, tokens_out) VALUES (?, ?, ?, ?)")
        .bind(&record.kind)
        .bind(&record.model)
        .bind(record.tokens_in)
        .bind(record.tokens_out)
        .execute(pool)
        .await?;
    Ok(())
}

/// 区间钳制到 SQLite datetime 可表达范围（[0, 9999-12-31]；i64::MAX 会令 datetime() 返回 NULL → 静默空结果，P-15）
/// `pub(crate)`：kv::cache_purge_before 同口径复用（两处喂 datetime() 的入口都必须钉）。
pub(crate) fn clamp_unix_range(from_unix: i64, to_unix: i64) -> (i64, i64) {
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
    use crate::common::*;

    #[test]
    fn test_record_usage_inserts_row() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                record_usage(&pool, &UsageRecord::new("question_gen", "m1", 11, 7))
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
                record_usage(&pool, &UsageRecord::new("llm", "m1", 100, 20))
                    .await
                    .expect("记账1");
                record_usage(&pool, &UsageRecord::new("jev", "jev-latest", 50, 10))
                    .await
                    .expect("记账2");
                record_usage(&pool, &UsageRecord::new("llm", "m1", 30, 5))
                    .await
                    .expect("记账3");
                let totals = usage_totals(&pool).await.expect("汇总");
                assert_eq!(totals.len(), 2, "按 kind 聚合");
                assert!(totals.contains(&("llm".to_string(), 130, 25)));
                assert!(totals.contains(&("jev".to_string(), 50, 10)));
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
                record_usage(&pool, &UsageRecord::new("question_gen", "m1", 100, 20))
                    .await
                    .expect("记账");
                record_usage(&pool, &UsageRecord::new("jev", "jev-latest", 50, 10))
                    .await
                    .expect("记账");
                record_usage(&pool, &UsageRecord::new("question_gen", "m1", 30, 5))
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
}
