//! 翻译工作台行（M5-W7，docs/02 §2 TranslateRepo）：translations/tx_favorites 两行面
use super::decode_payload;
use crate::*;
use ambition_core::Translation;

/// 落译文（tx_id = 内容哈希（text+dir+glossary）——同文同向幂等 upsert，与 llm_cache 同源=缓存命中，docs/11 §4）
pub async fn insert_translation(pool: &SqlitePool, tx: &Translation) -> Result<(), sqlx::Error> {
    let payload =
        serde_json::to_string(tx).map_err(|err| sqlx::Error::Decode(err.to_string().into()))?;
    let dir = format!("{}>{}", tx.dir.from.0, tx.dir.to.0);
    sqlx::query(
        "INSERT INTO translations (tx_id, dir, source, target, payload, created_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(tx_id) DO UPDATE SET target = excluded.target, payload = excluded.payload",
    )
    .bind(&tx.tx_id)
    .bind(dir)
    .bind(&tx.source)
    .bind(&tx.target)
    .bind(payload)
    .bind(ambition_core::now_unix())
    .execute(pool)
    .await?;
    Ok(())
}

/// 按 id 载译文（to_practice 取原文句；坏 payload 显式 Decode 上抛）
pub async fn get_translation(
    pool: &SqlitePool,
    tx_id: &str,
) -> Result<Option<Translation>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT payload FROM translations WHERE tx_id = ?")
        .bind(tx_id)
        .fetch_optional(pool)
        .await?;
    row.map(|(payload,)| {
        super::decode_payload::<Translation>(
            &payload,
            &format!("translation payload 损坏（{tx_id}）"),
        )
    })
    .transpose()
}

/// 检索/历史（q 模糊命中 source/target；None = 近 limit 全量倒序——history 与搜索共用面）
pub async fn search_translations(
    pool: &SqlitePool,
    q: Option<&str>,
    limit: i64,
) -> Result<Vec<Translation>, sqlx::Error> {
    let rows: Vec<(String,)> = match q {
        Some(needle) => {
            sqlx::query_as(
                "SELECT payload FROM translations WHERE source LIKE ? OR target LIKE ?
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(format!("%{needle}%"))
            .bind(format!("%{needle}%"))
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as("SELECT payload FROM translations ORDER BY created_at DESC LIMIT ?")
                .bind(limit)
                .fetch_all(pool)
                .await?
        }
    };
    rows.iter()
        .map(|(payload,)| {
            decode_payload::<Translation>(payload, "translation payload 损坏（历史行）")
        })
        .collect()
}

/// 收藏开关（on = upsert、off = delete；重复切换幂等）
pub async fn favorite_tx(pool: &SqlitePool, tx_id: &str, on: bool) -> Result<(), sqlx::Error> {
    if on {
        sqlx::query(
            "INSERT INTO tx_favorites (tx_id, created_at) VALUES (?, ?)
             ON CONFLICT(tx_id) DO NOTHING",
        )
        .bind(tx_id)
        .bind(ambition_core::now_unix())
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM tx_favorites WHERE tx_id = ?")
            .bind(tx_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// 收藏列表（tx_favorites 反连 translations；未落库的收藏行自然不出现）
pub async fn favorite_translations(pool: &SqlitePool) -> Result<Vec<Translation>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT t.payload FROM tx_favorites f JOIN translations t ON t.tx_id = f.tx_id
         ORDER BY f.created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|(payload,)| {
            decode_payload::<Translation>(payload, "translation payload 损坏（收藏行）")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{LangTag, TranslationDirection};

    fn tx(tx_id: &str, source: &str, target: &str) -> Translation {
        Translation {
            tx_id: tx_id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            dir: TranslationDirection {
                from: LangTag("zh-Hans".into()),
                to: LangTag::english(),
            },
            model: "m1".to_string(),
            glossary_hits: 1,
        }
    }

    #[tokio::test]
    async fn test_insert_translation_upserts_same_tx_id() {
        let pool = crate::common::fixtures::memory_pool().await;
        insert_translation(&pool, &tx("tx-1", "我喜欢春天", "I like spring"))
            .await
            .expect("首插");
        let updated = tx("tx-1", "我喜欢春天", "I love spring");
        insert_translation(&pool, &updated)
            .await
            .expect("同文同向覆盖");
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM translations")
            .fetch_one(&pool)
            .await
            .expect("计数");
        assert_eq!(n, 1, "tx_id 内容哈希幂等 upsert");
        let got = search_translations(&pool, None, 10).await.expect("检索");
        assert_eq!(got[0].target, "I love spring", "目标已覆盖");
    }

    #[tokio::test]
    async fn test_search_translations_matches_source_or_target() {
        let pool = crate::common::fixtures::memory_pool().await;
        insert_translation(&pool, &tx("tx-1", "我喜欢春天", "I like spring"))
            .await
            .expect("插1");
        insert_translation(&pool, &tx("tx-2", "剪纸艺术", "paper-cutting art"))
            .await
            .expect("插2");
        let hit = search_translations(&pool, Some("春天"), 10)
            .await
            .expect("命中");
        assert_eq!(hit.len(), 1, "source 命中");
        let hit2 = search_translations(&pool, Some("paper"), 10)
            .await
            .expect("命中");
        assert_eq!(hit2.len(), 1, "target 命中");
        let all = search_translations(&pool, None, 10).await.expect("全量");
        assert_eq!(all.len(), 2, "None = 全量倒序");
    }

    #[tokio::test]
    async fn test_favorite_tx_toggles_idempotently() {
        let pool = crate::common::fixtures::memory_pool().await;
        insert_translation(&pool, &tx("tx-1", "a", "A"))
            .await
            .expect("插");
        favorite_tx(&pool, "tx-1", true).await.expect("收藏");
        favorite_tx(&pool, "tx-1", true)
            .await
            .expect("重复收藏幂等");
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tx_favorites")
            .fetch_one(&pool)
            .await
            .expect("计数");
        assert_eq!(n, 1, "重复 on 不增行");
        favorite_tx(&pool, "tx-1", false).await.expect("取消");
        favorite_tx(&pool, "tx-1", false)
            .await
            .expect("重复 off 幂等");
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tx_favorites")
            .fetch_one(&pool)
            .await
            .expect("计数");
        assert_eq!(n, 0, "取消后无行");
    }

    #[tokio::test]
    async fn test_favorite_translations_returns_favorited_only() {
        let pool = crate::common::fixtures::memory_pool().await;
        insert_translation(&pool, &tx("tx-1", "a", "A"))
            .await
            .expect("插1");
        insert_translation(&pool, &tx("tx-2", "b", "B"))
            .await
            .expect("插2");
        favorite_tx(&pool, "tx-2", true).await.expect("只藏 tx-2");
        let list = favorite_translations(&pool).await.expect("收藏列表");
        assert_eq!(list.len(), 1, "仅收藏行");
        assert_eq!(list[0].tx_id, "tx-2");
        assert_eq!(list[0].target, "B", "payload 完整往返");
    }

    #[tokio::test]
    async fn test_get_translation_roundtrips_and_misses_none() {
        let pool = crate::common::fixtures::memory_pool().await;
        insert_translation(&pool, &tx("tx-9", "典故", "allusion"))
            .await
            .expect("插");
        let got = get_translation(&pool, "tx-9")
            .await
            .expect("载")
            .expect("存在");
        assert_eq!(got.source, "典故");
        assert!(
            get_translation(&pool, "nope").await.expect("查").is_none(),
            "未知 id → None"
        );
    }
}
