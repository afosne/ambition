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

/// 新作文（insert 行 DTO；essay_id = engine 内容哈希生成，docs/02 §2 EssayRepo）
#[derive(Debug, Clone, PartialEq)]
pub struct NewEssay {
    pub essay_id: String,
    pub qid: Option<String>,
    pub prompt: String,
    pub genre: String,
    pub body: String,
    pub word_count: i64,
    pub scaffold_stage: String,
    pub template_id: String,
    pub created_at: i64,
}

/// 落作文 +（可选）批改结果两行（同一事务；grade 编码失败 = 显式 Decode 上抛 P-4，不半提交）
pub async fn insert_essay(
    pool: &SqlitePool,
    essay: &NewEssay,
    grade: Option<&GradeResult>,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO essays (essay_id, qid, prompt, genre, body, word_count, scaffold_stage, template_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&essay.essay_id)
    .bind(&essay.qid)
    .bind(&essay.prompt)
    .bind(&essay.genre)
    .bind(&essay.body)
    .bind(essay.word_count)
    .bind(&essay.scaffold_stage)
    .bind(&essay.template_id)
    .bind(essay.created_at)
    .execute(&mut *tx)
    .await?;
    if let Some(grade) = grade {
        let payload = serde_json::to_string(grade)
            .map_err(|err| sqlx::Error::Decode(format!("essay grade encode: {err}").into()))?;
        sqlx::query("INSERT INTO essay_grades (essay_id, payload, graded_at) VALUES (?, ?, ?)")
            .bind(&essay.essay_id)
            .bind(payload)
            .bind(essay.created_at)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 作文历史行（grade 自 essay_grades 左连；未批 = None）
#[derive(Debug, Clone, PartialEq)]
pub struct EssayHistoryRow {
    pub essay_id: String,
    pub body: String,
    pub word_count: i64,
    pub created_at: i64,
    pub grade: Option<GradeResult>,
}

/// 同 prompt 最近 limit 篇（批改闭环回看；坏 grade payload 显式 Decode 上抛 P-4）
pub async fn essay_history(
    pool: &SqlitePool,
    prompt: &str,
    limit: i64,
) -> Result<Vec<EssayHistoryRow>, sqlx::Error> {
    let rows: Vec<(String, String, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT e.essay_id, e.body, e.word_count, e.created_at, g.payload
         FROM essays e LEFT JOIN essay_grades g ON g.essay_id = e.essay_id
         WHERE e.prompt = ? ORDER BY e.created_at DESC LIMIT ?",
    )
    .bind(prompt)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (essay_id, body, word_count, created_at, payload) in rows {
        let grade = match payload {
            Some(raw) => Some(decode_payload::<GradeResult>(
                &raw,
                &format!("essay grade payload 损坏（{essay_id}）"),
            )?),
            None => None,
        };
        out.push(EssayHistoryRow {
            essay_id,
            body,
            word_count,
            created_at,
            grade,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::{memory_pool, sample_grade};

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

    fn new_essay(essay_id: &str, prompt: &str, created_at: i64) -> NewEssay {
        NewEssay {
            essay_id: essay_id.into(),
            qid: Some("q1".into()),
            prompt: prompt.into(),
            genre: "argument".into(),
            body: "Nowadays more students write essays with tools.".into(),
            word_count: 9,
            scaffold_stage: "half".into(),
            template_id: "cet4_argument_procon".into(),
            created_at,
        }
    }

    #[tokio::test]
    async fn test_insert_essay_persists_row_and_optional_grade() {
        let pool = memory_pool().await;
        insert_essay(&pool, &new_essay("e1", "P-a", 100), None)
            .await
            .expect("无批改插入");
        let (essays, grades): (i64, i64) = (
            sqlx::query_scalar("SELECT count(*) FROM essays")
                .fetch_one(&pool)
                .await
                .expect("essay 计数"),
            sqlx::query_scalar("SELECT count(*) FROM essay_grades")
                .fetch_one(&pool)
                .await
                .expect("grade 计数"),
        );
        assert_eq!((essays, grades), (1, 0), "只落 essay 行");

        let grade = sample_grade(ambition_core::Verdict::Correct);
        insert_essay(&pool, &new_essay("e2", "P-a", 200), Some(&grade))
            .await
            .expect("带批改插入");
        let grades: i64 = sqlx::query_scalar("SELECT count(*) FROM essay_grades")
            .fetch_one(&pool)
            .await
            .expect("grade 计数");
        assert_eq!(grades, 1, "essay_grades 同事务落行");
    }

    #[tokio::test]
    async fn test_essay_history_joins_grade_and_orders_desc() {
        let pool = memory_pool().await;
        insert_essay(&pool, &new_essay("h1", "以绿色校园为题", 100), None)
            .await
            .expect("h1");
        let grade = sample_grade(ambition_core::Verdict::Partial);
        insert_essay(&pool, &new_essay("h2", "以绿色校园为题", 200), Some(&grade))
            .await
            .expect("h2");
        insert_essay(&pool, &new_essay("h3", "另一题目", 300), None)
            .await
            .expect("h3");

        let rows = essay_history(&pool, "以绿色校园为题", 10)
            .await
            .expect("历史");
        assert_eq!(rows.len(), 2, "按 prompt 过滤");
        assert_eq!(rows[0].essay_id, "h2", "最近在前（desc）");
        assert!(rows[0].grade.is_some(), "已批行带 grade");
        assert_eq!(rows[1].essay_id, "h1");
        assert!(rows[1].grade.is_none(), "未批 = None");

        let limited = essay_history(&pool, "以绿色校园为题", 1)
            .await
            .expect("limit");
        assert_eq!(limited.len(), 1, "limit 生效");
    }
}
