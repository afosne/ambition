//! 题库仓储：insert/get/list_question + provenance 解析（docs/02 §1）

use crate::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{BlankItem, Provenance, QuestionKind};
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
}
