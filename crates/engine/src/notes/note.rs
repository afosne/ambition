//! 笔记门面（M6-W1，docs/03 §6 note_*）：upsert/读/检索/反查/md 导出——FTS+双链落库单写者=store::note
use crate::*;
use ambition_core::{LinkRef, Note, NoteId, NoteKind};

impl Engine {
    /// 存笔记（ID=uuid v7、时间引擎生成；FTS/双链同写单写者——md 导出走 note_export_md，文件落盘随同步波次 docs/03 §45）
    pub async fn note_upsert(
        &self,
        title: String,
        body_md: String,
        kind: NoteKind,
        links: Vec<LinkRef>,
    ) -> Result<Note, EngineError> {
        let now = ambition_core::now_unix();
        let note = Note {
            note_id: NoteId::new_v7(),
            kind,
            title,
            body_md,
            links,
            created_at: now,
            updated_at: now,
        };
        ambition_store::upsert_note(&self.pool, &note).await?;
        Ok(note)
    }

    /// 取笔记（None=不存在）
    pub async fn note_get(&self, note_id: &str) -> Result<Option<Note>, EngineError> {
        Ok(ambition_store::get_note(&self.pool, note_id).await?)
    }

    /// FTS 检索（q 进 MATCH；语法错退化 LIKE 内容不静默——store 单写者口径 docs/12 §1 search）
    pub async fn note_search(&self, q: &str, limit: u32) -> Result<Vec<Note>, EngineError> {
        Ok(ambition_store::search_notes(&self.pool, q, i64::from(limit)).await?)
    }

    /// 反查（哪些笔记链接到该 ref——词↔题互跳的回程，docs/12 §1 backlinks）
    pub async fn note_backlinks(
        &self,
        ref_kind: &str,
        ref_value: &str,
    ) -> Result<Vec<Note>, EngineError> {
        Ok(ambition_store::backlinks(&self.pool, ref_kind, ref_value).await?)
    }

    /// md 导出（YAML front-matter + 正文——纯 Markdown 可直接同步网盘，docs/03 §45）
    pub fn note_export_md(&self, note: &Note) -> String {
        ambition_store::note_export_md(note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn word_note(engine: &Engine) -> Note {
        engine
            .note_upsert(
                "take".into(),
                "# take\n".into(),
                NoteKind::Word,
                vec![LinkRef::Qid(ambition_core::Qid("q-1".into()))],
            )
            .await
            .expect("存笔记")
    }

    #[tokio::test]
    async fn test_note_upsert_writes_fts_and_links() {
        let engine = Engine::open_memory().await.expect("开内存库");
        let note = word_note(&engine).await;
        assert_eq!(note.kind, NoteKind::Word, "类型落位");
        assert_eq!(note.links.len(), 1, "双链随载");
    }

    #[tokio::test]
    async fn test_note_get_roundtrips_single_note() {
        let engine = Engine::open_memory().await.expect("开内存库");
        let note = word_note(&engine).await;
        let got = engine
            .note_get(&note.note_id.0)
            .await
            .expect("取")
            .expect("存在");
        assert_eq!(got.title, "take");
        assert!(
            engine.note_get("missing").await.expect("取").is_none(),
            "未命中 None"
        );
    }

    #[tokio::test]
    async fn test_note_search_hits_title_and_body() {
        let engine = Engine::open_memory().await.expect("开内存库");
        word_note(&engine).await;
        let hits = engine.note_search("take", 10).await.expect("检索");
        assert_eq!(hits.len(), 1, "FTS 命中标题");
        assert!(
            engine
                .note_search("no-such-term", 10)
                .await
                .expect("检索")
                .is_empty(),
            "未命中空"
        );
    }

    #[tokio::test]
    async fn test_note_backlinks_finds_incoming_refs() {
        let engine = Engine::open_memory().await.expect("开内存库");
        word_note(&engine).await;
        let backs = engine.note_backlinks("qid", "q-1").await.expect("反查");
        assert_eq!(backs.len(), 1, "词笔记反查到题链接");
        assert!(
            engine
                .note_backlinks("qid", "q-none")
                .await
                .expect("反查")
                .is_empty(),
            "无链接空"
        );
    }

    #[test]
    fn test_note_export_md_emits_front_matter() {
        let note = Note {
            note_id: NoteId::new_v7(),
            kind: NoteKind::Word,
            title: "take".into(),
            body_md: "# take\n".into(),
            links: vec![],
            created_at: 1_000,
            updated_at: 2_000,
        };
        let md = ambition_store::note_export_md(&note);
        assert!(md.starts_with("---\n"), "YAML 头");
        assert!(md.contains("id: "), "带 id");
        assert!(md.ends_with("# take\n"), "正文在尾");
    }
}
