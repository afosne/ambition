//! srs_queue 卡片 CRUD：upsert_card/get_card/due_cards/wrong_cards/list_cards（docs/02 §2 迁移 v3）
//! 单写者 = 08-adaptive（经 engine：on_attempt/review_answer）；本模块只做存取，不参与调度决策。

use super::*;
use ambition_srs::Card;

/// 幂等写卡（INSERT OR REPLACE：同 card_id 覆盖 = 内容寻址幂等入卡，docs/02 §1）
pub async fn upsert_card(pool: &SqlitePool, card: &Card) -> Result<(), sqlx::Error> {
    let payload = serde_json::to_string(card).unwrap_or_default();
    let source = serde_json::to_string(&card.source).unwrap_or_default();
    sqlx::query(
        "INSERT INTO srs_queue (card_id, source, payload, due_at, wrong_count)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(card_id) DO UPDATE SET
           source = excluded.source,
           payload = excluded.payload,
           due_at = excluded.due_at,
           wrong_count = excluded.wrong_count",
    )
    .bind(&card.card_id.0)
    .bind(&source)
    .bind(payload)
    .bind(card.due_at)
    .bind(card.wrong_count as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// 按 card_id 读卡（不存在 → None）
pub async fn get_card(pool: &SqlitePool, card_id: &str) -> Result<Option<Card>, sqlx::Error> {
    let payload: Option<String> =
        sqlx::query_scalar("SELECT payload FROM srs_queue WHERE card_id = ?")
            .bind(card_id)
            .fetch_optional(pool)
            .await?;
    parse_card_payload(payload.as_deref())
}

/// 到期卡（due_at <= now；错词多者提权 + 到期早者优先，docs/08 §1 排序口径前置）
pub async fn due_cards(pool: &SqlitePool, now: i64) -> Result<Vec<Card>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT card_id, payload FROM srs_queue WHERE due_at <= ? ORDER BY wrong_count DESC, due_at ASC",
    )
    .bind(now)
    .fetch_all(pool)
    .await?;
    collect_cards(rows)
}

/// 错词卡（wrong_count > 0；错词本入口，docs/03 §2 wrong_wordbook）
pub async fn wrong_cards(pool: &SqlitePool) -> Result<Vec<Card>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT card_id, payload FROM srs_queue WHERE wrong_count > 0 ORDER BY wrong_count DESC",
    )
    .fetch_all(pool)
    .await?;
    collect_cards(rows)
}

/// 全部卡（due_queue 交错/截断在 srs 纯函数层做，engine 取全量组装 ReviewItem）
pub async fn list_cards(pool: &SqlitePool) -> Result<Vec<Card>, sqlx::Error> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT card_id, payload FROM srs_queue ORDER BY card_id")
            .fetch_all(pool)
            .await?;
    collect_cards(rows)
}

/// 载荷解析（坏行 = Decode 错误显式上抛，不静默丢卡）
fn parse_card_payload(payload: Option<&str>) -> Result<Option<Card>, sqlx::Error> {
    let Some(payload) = payload else {
        return Ok(None);
    };
    let card: Card = serde_json::from_str(payload).map_err(|err| {
        sqlx::Error::Decode(format!("srs payload 损坏（载荷截断或版本漂移）: {err}").into())
    })?;
    Ok(Some(card))
}

/// 批量解析卡行（坏行显式报错，见 parse_card_payload）
fn collect_cards(rows: Vec<(String, String)>) -> Result<Vec<Card>, sqlx::Error> {
    let mut cards = Vec::with_capacity(rows.len());
    for (_card_id, payload) in rows {
        if let Some(card) = parse_card_payload(Some(&payload))? {
            cards.push(card);
        }
    }
    Ok(cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::CardSource;
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

    fn sample_card(now: i64) -> Card {
        let mut card = Card::new(
            CardSource::Question {
                qid: ambition_core::Qid("q1".into()),
            },
            now,
        );
        card.wrong_count = 2;
        card
    }

    #[test]
    fn test_upsert_card_roundtrips_and_replaces_idempotently() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let mut card = sample_card(1_000);
                upsert_card(&pool, &card).await.expect("写卡");
                let got = get_card(&pool, &card.card_id.0)
                    .await
                    .expect("读卡")
                    .expect("存在");
                assert_eq!(got, card, "Card JSON 往返无损");
                // 同 id 覆盖（幂等入卡后提权）
                card.due_at = 2_000;
                card.wrong_count = 3;
                upsert_card(&pool, &card).await.expect("覆盖");
                let got = get_card(&pool, &card.card_id.0)
                    .await
                    .expect("读卡")
                    .expect("存在");
                assert_eq!(got.due_at, 2_000, "覆盖生效");
                assert_eq!(got.wrong_count, 3);
                let all = list_cards(&pool).await.expect("全量");
                assert_eq!(all.len(), 1, "同 id 不重复");
            });
    }

    #[test]
    fn test_due_cards_filters_by_now() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let due = sample_card(1_000); // wrong=2
                let mut later = Card::new(
                    CardSource::WordSense {
                        word_id: "bank".into(),
                        sense_id: 1,
                    },
                    5_000,
                );
                later.due_at = 5_000;
                upsert_card(&pool, &due).await.expect("写");
                upsert_card(&pool, &later).await.expect("写");
                let dued = due_cards(&pool, 2_000).await.expect("到期");
                assert_eq!(dued.len(), 1, "只取 due_at<=now");
                assert_eq!(dued[0].card_id, due.card_id);
                let none = due_cards(&pool, 500).await.expect("更早");
                assert!(none.is_empty(), "未到期为空");
            });
    }

    #[test]
    fn test_wrong_cards_orders_by_count_desc() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                let due = sample_card(1_000); // wrong=2
                let mut future_wrong = Card::new(
                    CardSource::Pattern {
                        pattern_id: "not_only".into(),
                    },
                    9_000,
                );
                future_wrong.wrong_count = 1;
                upsert_card(&pool, &due).await.expect("写");
                upsert_card(&pool, &future_wrong).await.expect("写");
                let wrong = wrong_cards(&pool).await.expect("错词");
                assert_eq!(wrong.len(), 2, "wrong_count>0 两枚");
                assert_eq!(wrong[0].wrong_count, 2, "错词多者在前");
            });
    }

    #[test]
    fn test_list_cards_returns_all_rows() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                upsert_card(&pool, &sample_card(1_000)).await.expect("写");
                let mut other = Card::new(
                    CardSource::WordSense {
                        word_id: "bank".into(),
                        sense_id: 2,
                    },
                    5_000,
                );
                other.due_at = 5_000;
                upsert_card(&pool, &other).await.expect("写");
                let all = list_cards(&pool).await.expect("全量");
                assert_eq!(all.len(), 2);
            });
    }

    #[test]
    fn test_parse_card_payload_reports_corrupt_row() {
        // 坏行显式 Decode 错误（不静默丢卡）
        let card = sample_card(1_000);
        let json = serde_json::to_string(&card).expect("序列化");
        let parsed = parse_card_payload(Some(&json))
            .expect("解析")
            .expect("存在");
        assert_eq!(parsed, card);
        assert!(
            parse_card_payload(Some("{broken")).is_err(),
            "坏载荷 → 显式 Err"
        );
        assert!(
            parse_card_payload(None).expect("None 解析").is_none(),
            "无载荷 → None"
        );
    }

    #[test]
    fn test_collect_cards_maps_rows_in_order() {
        let card = sample_card(1_000);
        let json = serde_json::to_string(&card).expect("序列化");
        let rows = vec![(card.card_id.0.clone(), json)];
        let cards = collect_cards(rows).expect("批量解析");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0], card);
        let bad = collect_cards(vec![("x".into(), "{bad".into())]);
        assert!(bad.is_err(), "坏行上抛不静默");
    }

    #[test]
    fn test_get_card_missing_returns_none() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let pool = memory_pool().await;
                assert!(
                    get_card(&pool, "nope").await.expect("查询").is_none(),
                    "不存在 → None"
                );
                assert!(due_cards(&pool, 0).await.expect("空队列").is_empty());
            });
    }
}
