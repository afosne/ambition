//! 写作三表仓（docs/02 §2 EssayRepo、迁移 v4；M5-W4 落模板面，essays/essay_grades 写入随 W5 批改闭环）
//! 坏载荷一律显式 `Decode` 上抛（P-4 口径，经 repo::decode_payload 收口），不静默丢行。

use ambition_writing::{Template, TemplateSource};

use super::decode_payload;
use crate::*;

/// source 枚举 → 列值（snake_case；禁 Debug 拼串，docs/01 §迁移版本化）
fn source_str(source: TemplateSource) -> &'static str {
    match source {
        TemplateSource::Builtin => "builtin",
        TemplateSource::Personal => "personal",
    }
}

/// 列值 → source（未知值防御性回 Builtin，不臆造 Personal）
fn parse_source(raw: &str) -> TemplateSource {
    match raw {
        "personal" => TemplateSource::Personal,
        _ => TemplateSource::Builtin,
    }
}

/// Upsert 模板（同 id 覆盖——个人模板覆盖内置同 id 由 engine 合并口径处理，docs/10 §1）
pub async fn upsert_template(pool: &SqlitePool, t: &Template) -> Result<(), sqlx::Error> {
    let payload = serde_json::to_string(t)
        .map_err(|err| sqlx::Error::Decode(format!("template encode: {err}").into()))?;
    sqlx::query(
        "INSERT INTO templates (template_id, track, genre, source, payload)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(template_id) DO UPDATE SET
           track = excluded.track, genre = excluded.genre,
           source = excluded.source, payload = excluded.payload",
    )
    .bind(&t.template_id)
    .bind(&t.track)
    .bind(&t.genre)
    .bind(source_str(t.source))
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

/// 全部已存模板（DB 内 = 个人/上新；按 id 升序，engine 再与内置合并）
pub async fn templates(pool: &SqlitePool) -> Result<Vec<Template>, sqlx::Error> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT template_id, source, payload FROM templates ORDER BY template_id")
            .fetch_all(pool)
            .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, source_raw, payload) in rows {
        let mut t: Template = decode_payload(&payload, &format!("template payload 损坏（{id}）"))?;
        // 列 source 为准（载荷内 source 与列漂移时不静默：以列为准重建）
        t.source = parse_source(&source_raw);
        out.push(t);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::memory_pool;

    fn demo_template(genre: &str, source: TemplateSource) -> Template {
        Template {
            template_id: "t-demo".into(),
            track: "cet4".into(),
            genre: genre.into(),
            skeleton: vec![],
            slots: vec![],
            phrases: vec![],
            anti_patterns: vec![],
            source,
        }
    }

    #[test]
    fn test_source_str_maps_enum_to_snake_case() {
        assert_eq!(source_str(TemplateSource::Builtin), "builtin");
        assert_eq!(source_str(TemplateSource::Personal), "personal");
    }

    #[test]
    fn test_parse_source_defaults_unknown_to_builtin() {
        assert_eq!(parse_source("personal"), TemplateSource::Personal);
        assert_eq!(parse_source("builtin"), TemplateSource::Builtin);
        assert_eq!(
            parse_source("mystery"),
            TemplateSource::Builtin,
            "未知不臆造 Personal"
        );
    }

    #[tokio::test]
    async fn test_upsert_template_writes_all_columns() {
        let pool = memory_pool().await;
        upsert_template(&pool, &demo_template("notice", TemplateSource::Personal))
            .await
            .expect("插入");
        let (track, genre, source): (String, String, String) = sqlx::query_as(
            "SELECT track, genre, source FROM templates WHERE template_id = 't-demo'",
        )
        .fetch_one(&pool)
        .await
        .expect("列读取");
        assert_eq!(
            (track.as_str(), genre.as_str(), source.as_str()),
            ("cet4", "notice", "personal"),
            "四列齐写（payload 另测）"
        );
    }

    #[tokio::test]
    async fn test_templates_upsert_roundtrips_and_overwrites_by_id() {
        let pool = memory_pool().await;
        upsert_template(&pool, &demo_template("argument", TemplateSource::Builtin))
            .await
            .expect("插入");
        upsert_template(&pool, &demo_template("letter", TemplateSource::Personal))
            .await
            .expect("同 id 覆盖");
        let list = templates(&pool).await.expect("列表");
        assert_eq!(list.len(), 1, "同 id 覆盖不新增行");
        assert_eq!(list[0].genre, "letter", "覆盖生效");
        assert_eq!(list[0].source, TemplateSource::Personal, "列 source 解析");
        assert_eq!(list[0].template_id, "t-demo");
    }
}
