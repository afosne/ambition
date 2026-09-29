//! 笔记仓（M6-W1，docs/02 §2 NoteRepo）：upsert（行 + FTS5 + 双链同写）/ 读 / FTS 检索 / 反查 / md 导出
use ambition_core::{LinkRef, Note, NoteKind};

use super::decode_payload;
use crate::*;

/// 笔记类型列名（notes.kind；snake_case 单词）
fn kind_str(kind: &NoteKind) -> &'static str {
    match kind {
        NoteKind::Word => "word",
        NoteKind::Question => "question",
        NoteKind::Grammar => "grammar",
        NoteKind::Phrase => "phrase",
        NoteKind::Free => "free",
    }
}

/// 双链行拆分（ref_kind/ref_value 两列——反查索引键）
fn link_parts(link: &LinkRef) -> (&'static str, String) {
    match link {
        LinkRef::Word(lemma) => ("word", lemma.clone()),
        LinkRef::Qid(qid) => ("qid", qid.0.clone()),
        LinkRef::Grammar(g) => ("grammar", g.clone()),
        LinkRef::Essay(e) => ("essay", e.clone()),
        LinkRef::Note(n) => ("note", n.0.clone()),
    }
}

/// upsert 笔记（同 id 覆盖：行 + FTS 行 + 双链行在同一事务重建——单写者口径，docs/12 §1）
pub async fn upsert_note(pool: &SqlitePool, note: &Note) -> Result<(), sqlx::Error> {
    let payload = serde_json::to_string(note)
        .map_err(|err| sqlx::Error::Decode(format!("note encode: {err}").into()))?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO notes (note_id, kind, title, body_md, payload, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(note_id) DO UPDATE SET
           kind = excluded.kind, title = excluded.title, body_md = excluded.body_md,
           payload = excluded.payload, updated_at = excluded.updated_at",
    )
    .bind(&note.note_id.0)
    .bind(kind_str(&note.kind))
    .bind(&note.title)
    .bind(&note.body_md)
    .bind(payload)
    .bind(note.created_at)
    .bind(note.updated_at)
    .execute(&mut *tx)
    .await?;
    // 双链重建（全删全插——upsert 幂等）
    sqlx::query("DELETE FROM note_links WHERE note_id = ?")
        .bind(&note.note_id.0)
        .execute(&mut *tx)
        .await?;
    for link in &note.links {
        let (ref_kind, ref_value) = link_parts(link);
        sqlx::query(
            "INSERT INTO note_links (note_id, ref_kind, ref_value, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&note.note_id.0)
        .bind(ref_kind)
        .bind(ref_value)
        .bind(note.created_at)
        .execute(&mut *tx)
        .await?;
    }
    // FTS 重建（fts5 行的 rowid 固定取 notes.rowid——删旧插新同 id 幂等）
    sqlx::query("DELETE FROM notes_fts WHERE rowid = (SELECT rowid FROM notes WHERE note_id = ?)")
        .bind(&note.note_id.0)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO notes_fts (rowid, title, body_md)
         SELECT rowid, title, body_md FROM notes WHERE note_id = ?",
    )
    .bind(&note.note_id.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// 读笔记（payload 整行；坏行显式 Decode 上抛 P-4）
pub async fn get_note(pool: &SqlitePool, note_id: &str) -> Result<Option<Note>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT payload FROM notes WHERE note_id = ?")
        .bind(note_id)
        .fetch_optional(pool)
        .await?;
    row.map(|(payload,)| {
        decode_payload::<Note>(&payload, &format!("note payload 损坏（{note_id}）"))
    })
    .transpose()
}

/// FTS 检索（MATCH 语法错（引号/保留词）→ 退化 LIKE——仅排序降级，内容不静默丢失）
async fn fts_or_like(
    pool: &SqlitePool,
    q: &str,
    limit: i64,
) -> Result<Vec<(String,)>, sqlx::Error> {
    let fts = sqlx::query_as::<_, (String,)>(
        "SELECT n.payload FROM notes_fts f JOIN notes n ON n.rowid = f.rowid
         WHERE notes_fts MATCH ? ORDER BY rank LIMIT ?",
    )
    .bind(q)
    .bind(limit)
    .fetch_all(pool)
    .await;
    match fts {
        Ok(rows) => Ok(rows),
        Err(sqlx::Error::Database(err)) => {
            tracing::warn!(error = %err.message(), q, "fts MATCH 语法错，退化 LIKE 检索");
            sqlx::query_as::<_, (String,)>(
                "SELECT payload FROM notes WHERE title LIKE ? OR body_md LIKE ?
                 ORDER BY updated_at DESC LIMIT ?",
            )
            .bind(format!("%{q}%"))
            .bind(format!("%{q}%"))
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        Err(err) => Err(err),
    }
}

/// 检索笔记（FTS 优先，语义同 docs/12 §1 search(FTS)）
pub async fn search_notes(
    pool: &SqlitePool,
    q: &str,
    limit: i64,
) -> Result<Vec<Note>, sqlx::Error> {
    let rows = fts_or_like(pool, q, limit).await?;
    rows.iter()
        .map(|(payload,)| decode_payload::<Note>(payload, "note payload 损坏（search）"))
        .collect()
}

/// 反查（哪些笔记链接到该值——docs/12 §1 backlinks）
pub async fn backlinks(
    pool: &SqlitePool,
    ref_kind: &str,
    ref_value: &str,
) -> Result<Vec<Note>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT n.payload FROM note_links l JOIN notes n ON n.note_id = l.note_id
         WHERE l.ref_kind = ? AND l.ref_value = ? ORDER BY n.updated_at DESC",
    )
    .bind(ref_kind)
    .bind(ref_value)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|(payload,)| decode_payload::<Note>(payload, "note payload 损坏（backlinks）"))
        .collect()
}

/// md 导出（front-matter + 正文——纯 Markdown 可直接同步网盘，docs/03 §7/§45）
pub fn note_export_md(note: &Note) -> String {
    format!(
        "---\nid: {}\ntitle: \"{}\"\nkind: {}\ncreated: {}\nupdated: {}\nlinks: {}\n---\n\n{}",
        note.note_id.0,
        note.title,
        kind_str(&note.kind),
        note.created_at,
        note.updated_at,
        note.links
            .iter()
            .map(|l| link_parts(l).1.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        note.body_md
    )
}

/// 按标题+类型查重（link_new_words 去重用——同型同题一行）
pub async fn find_note_by_title(
    pool: &SqlitePool,
    title: &str,
    kind: &str,
) -> Result<Option<Note>, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT payload FROM notes WHERE title = ? AND kind = ? LIMIT 1")
            .bind(title)
            .bind(kind)
            .fetch_optional(pool)
            .await?;
    row.map(|(payload,)| decode_payload::<Note>(&payload, "note payload 损坏（title 查重）"))
        .transpose()
}

#[cfg(test)]
mod tests {
    use ambition_core::{LinkRef, Note, NoteId, NoteKind, Qid};

    use super::*;
    use crate::common::fixtures::memory_pool;

    #[tokio::test]
    async fn test_upsert_note_writes_and_overwrites_idempotently() {
        let pool = memory_pool().await;
        let note = Note {
            note_id: NoteId("n-up".into()),
            kind: NoteKind::Word,
            title: "bank".into(),
            body_md: "银行".into(),
            links: vec![LinkRef::Word("bank".into())],
            created_at: 10,
            updated_at: 10,
        };
        upsert_note(&pool, &note).await.expect("首次写入");
        let got = get_note(&pool, "n-up").await.expect("读").expect("存在");
        assert_eq!(got.title, "bank");
        let mut updated = note.clone();
        updated.body_md = "河岸".into();
        updated.updated_at = 20;
        upsert_note(&pool, &updated).await.expect("覆盖");
        let got = get_note(&pool, "n-up").await.expect("读").expect("仍在");
        assert_eq!(got.body_md, "河岸");
        assert_eq!(got.updated_at, 20);
    }

    #[test]
    fn test_kind_str_maps_all_five_note_kinds() {
        assert_eq!(kind_str(&NoteKind::Word), "word");
        assert_eq!(kind_str(&NoteKind::Question), "question");
        assert_eq!(kind_str(&NoteKind::Grammar), "grammar");
        assert_eq!(kind_str(&NoteKind::Phrase), "phrase");
        assert_eq!(kind_str(&NoteKind::Free), "free");
    }

    #[test]
    fn test_link_parts_categorizes_each_reference() {
        assert_eq!(
            link_parts(&LinkRef::Word("spring".into())),
            ("word", "spring".to_string())
        );
        assert_eq!(
            link_parts(&LinkRef::Qid(Qid("q1".into()))),
            ("qid", "q1".to_string())
        );
        assert_eq!(
            link_parts(&LinkRef::Essay("e1".into())),
            ("essay", "e1".to_string())
        );
    }

    #[test]
    fn test_note_export_md_has_front_matter_and_body() {
        let note = Note {
            note_id: NoteId("n1".into()),
            kind: NoteKind::Word,
            title: "spring".into(),
            body_md: "# 一词\n义项…".into(),
            links: vec![LinkRef::Word("spring".into())],
            created_at: 1_000,
            updated_at: 2_000,
        };
        let md = note_export_md(&note);
        assert!(md.starts_with("---\nid: n1"), "front-matter 起手");
        assert!(md.contains("kind: word"));
        assert!(md.ends_with("# 一词\n义项…"), "正文在 fence 后");
    }

    #[tokio::test]
    async fn test_search_notes_upsert_writes_fts_and_links() {
        let pool = memory_pool().await;
        let note = Note {
            note_id: NoteId::new_v7(),
            kind: NoteKind::Phrase,
            title: "take sth into account".into(),
            body_md: "搭配：把……考虑在内；避免 brute\"force 这类搭配错误。".into(),
            links: vec![
                LinkRef::Word("consider".into()),
                LinkRef::Qid(Qid("q9".into())),
            ],
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
        };
        upsert_note(&pool, &note).await.expect("upsert");
        let hits = search_notes(&pool, "brute", 10).await.expect("FTS 检索");
        assert_eq!(hits.len(), 1, "FTS 命中正文词（title/body 索引）");
        let mut updated = note.clone();
        updated.title = "take into account".into();
        upsert_note(&pool, &updated).await.expect("覆盖");
        let hits = search_notes(&pool, "take", 10).await.expect("覆盖后检索");
        assert_eq!(hits.len(), 1, "覆盖不增行");
        assert_eq!(hits[0].title, "take into account", "标题已更新");
    }

    #[tokio::test]
    async fn test_get_note_roundtrips_and_misses_unknown() {
        let pool = memory_pool().await;
        let note = Note {
            note_id: NoteId("n-rt".into()),
            kind: NoteKind::Grammar,
            title: "非谓语".into(),
            body_md: "to do / doing / done".into(),
            links: vec![LinkRef::Grammar("非谓语".into())],
            created_at: 42,
            updated_at: 43,
        };
        upsert_note(&pool, &note).await.expect("写");
        let got = get_note(&pool, "n-rt").await.expect("读").expect("存在");
        assert_eq!(got, note, "整行往返一致");
        assert!(
            get_note(&pool, "n-miss").await.expect("读").is_none(),
            "未知 id → None"
        );
    }

    #[tokio::test]
    async fn test_find_note_by_title_matches_kind_and_misses_other_kind() {
        let pool = memory_pool().await;
        let note = Note {
            note_id: NoteId("n-t".into()),
            kind: NoteKind::Word,
            title: "spring".into(),
            body_md: "春".into(),
            links: vec![],
            created_at: 1,
            updated_at: 1,
        };
        upsert_note(&pool, &note).await.expect("写");
        let hit = find_note_by_title(&pool, "spring", "word")
            .await
            .expect("查")
            .expect("命中");
        assert_eq!(hit.note_id, NoteId("n-t".into()));
        assert!(
            find_note_by_title(&pool, "spring", "phrase")
                .await
                .expect("查")
                .is_none(),
            "类型不同不算重复"
        );
        assert!(
            find_note_by_title(&pool, "missing", "word")
                .await
                .expect("查")
                .is_none(),
            "无题 → None"
        );
    }

    #[tokio::test]
    async fn test_fts_or_like_falls_back_when_syntax_breaks() {
        let pool = memory_pool().await;
        let note = Note {
            note_id: NoteId("n-q".into()),
            kind: NoteKind::Free,
            title: "quote test".into(),
            body_md: "avoid brute\"force wording".into(),
            links: vec![],
            created_at: 1,
            updated_at: 1,
        };
        upsert_note(&pool, &note).await.expect("写");
        let rows = fts_or_like(&pool, "brute\"", 10).await.expect("退化检索");
        assert_eq!(rows.len(), 1, "LIKE 命中含特殊符正文");
    }

    #[tokio::test]
    async fn test_backlinks_finds_notes_pointing_at_value() {
        let pool = memory_pool().await;
        let n1 = Note {
            note_id: NoteId("n1".into()),
            kind: NoteKind::Word,
            title: "consider".into(),
            body_md: "考虑；把……考虑在内".into(),
            links: vec![LinkRef::Word("consider".into())],
            created_at: 1,
            updated_at: 1,
        };
        upsert_note(&pool, &n1).await.expect("写1");
        let mut n2 = n1.clone();
        n2.note_id = NoteId("n2".into());
        n2.links = vec![LinkRef::Essay("e-1".into())];
        upsert_note(&pool, &n2).await.expect("写2");
        let hits = backlinks(&pool, "word", "consider").await.expect("反查");
        assert_eq!(hits.len(), 1, "只有 n1 指向该词");
        assert_eq!(hits[0].note_id, NoteId("n1".into()));
        assert!(
            backlinks(&pool, "word", "missing")
                .await
                .expect("空")
                .is_empty(),
            "无引用 → 空"
        );
    }
}
