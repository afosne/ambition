//! 日记仓（M6-W1，docs/02 §2 DiaryRepo）：一日一篇 upsert + 读 + 列表
use ambition_core::{DiaryEntry, DiaryReview};

use crate::*;

/// 写日记（date 唯一——同日覆盖保留首条 entry_id；review 可空，docs/12 §1 diary_write）
pub async fn diary_write(pool: &SqlitePool, entry: &DiaryEntry) -> Result<(), sqlx::Error> {
    let review = match &entry.review {
        Some(review) => Some(
            serde_json::to_string(review)
                .map_err(|err| sqlx::Error::Decode(format!("diary review encode: {err}").into()))?,
        ),
        None => None,
    };
    sqlx::query(
        "INSERT INTO diary_entries (entry_id, date, body, review)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(date) DO UPDATE SET body = excluded.body, review = excluded.review",
    )
    .bind(&entry.entry_id.0)
    .bind(&entry.date)
    .bind(&entry.body)
    .bind(review)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn diary_set_review(
    pool: &SqlitePool,
    date: &str,
    review: &DiaryReview,
) -> Result<bool, sqlx::Error> {
    let payload = serde_json::to_string(review)
        .map_err(|err| sqlx::Error::Decode(format!("diary review encode: {err}").into()))?;
    let result = sqlx::query("UPDATE diary_entries SET review=? WHERE date=?")
        .bind(payload)
        .bind(date)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// 读某日日记（None = 当日未写）
pub async fn diary_read(pool: &SqlitePool, date: &str) -> Result<Option<DiaryEntry>, sqlx::Error> {
    let row: Option<(String, String, String, Option<String>)> =
        sqlx::query_as("SELECT entry_id, date, body, review FROM diary_entries WHERE date = ?")
            .bind(date)
            .fetch_optional(pool)
            .await?;
    row.map(|(entry_id, date, body, review)| {
        let review = match review {
            Some(raw) => Some(serde_json::from_str::<DiaryReview>(&raw).map_err(|err| {
                sqlx::Error::Decode(format!("diary review 损坏（{date}）: {err}").into())
            })?),
            None => None,
        };
        Ok(DiaryEntry {
            entry_id: ambition_core::EntryId(entry_id),
            date,
            body,
            review,
        })
    })
    .transpose()
}

/// 列表（近 limit 天，日期倒序——画像/复看入口）
pub async fn list_diary(pool: &SqlitePool, limit: i64) -> Result<Vec<DiaryEntry>, sqlx::Error> {
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT entry_id, date, body, review FROM diary_entries ORDER BY date DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(entry_id, date, body, review)| {
            let review = match review {
                Some(raw) => Some(serde_json::from_str::<DiaryReview>(&raw).map_err(|err| {
                    sqlx::Error::Decode(format!("diary review 损坏（{date}）: {err}").into())
                })?),
                None => None,
            };
            Ok(DiaryEntry {
                entry_id: ambition_core::EntryId(entry_id),
                date,
                body,
                review,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ambition_core::{DiaryEntry, DiaryReview, EntryId};

    use super::*;
    use crate::common::fixtures::memory_pool;

    fn entry(date: &str, body: &str) -> DiaryEntry {
        DiaryEntry {
            entry_id: EntryId::new_v7(),
            date: date.to_string(),
            body: body.to_string(),
            review: None,
        }
    }

    #[tokio::test]
    async fn test_diary_write_upserts_one_row_per_date() {
        let pool = memory_pool().await;
        let first = entry("2026-09-29", "第一天");
        let first_id = first.entry_id.clone();
        diary_write(&pool, &first).await.expect("写");
        let mut same_day = entry("2026-09-29", "第一天（改）");
        same_day.entry_id = EntryId("fixed".into());
        diary_write(&pool, &same_day).await.expect("同日覆盖");
        let got = diary_read(&pool, "2026-09-29")
            .await
            .expect("读")
            .expect("存在");
        assert_eq!(got.body, "第一天（改）", "同日覆盖正文");
        assert_eq!(got.entry_id, first_id, "同日覆盖保留首条 entry_id");
        let rows = list_diary(&pool, 10).await.expect("列");
        assert_eq!(rows.len(), 1, "一日一篇不增行");
    }

    #[tokio::test]
    async fn test_diary_read_misses_unknown_date() {
        let pool = memory_pool().await;
        assert!(diary_read(&pool, "2026-01-01").await.expect("读").is_none());
    }

    #[tokio::test]
    async fn test_diary_set_review_saves_existing_entry_only() {
        let pool = memory_pool().await;
        diary_write(&pool, &entry("2026-09-29", "entry"))
            .await
            .expect("写日记");
        let review = DiaryReview {
            corrected_md: "corrected".into(),
            upgrades: vec![],
            encouragement: "good".into(),
            model: "test".into(),
            reviewed_at: 1,
        };
        assert!(
            diary_set_review(&pool, "2026-09-29", &review)
                .await
                .expect("更新")
        );
        assert!(
            !diary_set_review(&pool, "1999-01-01", &review)
                .await
                .expect("无行")
        );
        assert_eq!(
            diary_read(&pool, "2026-09-29")
                .await
                .unwrap()
                .unwrap()
                .review,
            Some(review)
        );
    }

    #[tokio::test]
    async fn test_list_diary_orders_desc_and_carries_review() {
        let pool = memory_pool().await;
        let mut e1 = entry("2026-09-28", "早");
        e1.review = Some(DiaryReview {
            corrected_md: "改".into(),
            upgrades: vec!["good→beneficial".into()],
            encouragement: "稳".into(),
            model: "m".into(),
            reviewed_at: 9,
        });
        diary_write(&pool, &e1).await.expect("写1");
        diary_write(&pool, &entry("2026-09-29", "晚"))
            .await
            .expect("写2");
        let rows = list_diary(&pool, 10).await.expect("列");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].date, "2026-09-29", "日期倒序");
        assert_eq!(
            rows[1].review.as_ref().expect("批注随行").encouragement,
            "稳",
            "批注随行走"
        );
    }
}
