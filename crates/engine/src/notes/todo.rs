//! 待办门面（M6-W1，docs/03 todo_* + docs/12 §1）：增列/清单/完成发事件/考前排期/每日清单/生词入笔记
use crate::*;
use ambition_core::{Note, NoteKind, Todo, TodoKind, TodoStatus};

impl Engine {
    /// 加待办（meta 空对象；due 可空 = 未排期）
    pub async fn todo_add(
        &self,
        kind: TodoKind,
        title: String,
        due: Option<i64>,
    ) -> Result<Todo, EngineError> {
        let todo = Todo {
            todo_id: ambition_core::TodoId::new_v7(),
            kind,
            title,
            due,
            status: TodoStatus::Pending,
            meta: "{}".to_string(),
            created_at: ambition_core::now_unix(),
        };
        ambition_store::add_todo(&self.pool, &todo).await?;
        Ok(todo)
    }

    /// 待办清单（include_done 过滤；due 空排最前、同 due 按创建序——docs/12 daily_checklist 数据源）
    pub async fn todo_list(
        &self,
        include_done: bool,
        limit: u32,
    ) -> Result<Vec<Todo>, EngineError> {
        Ok(ambition_store::list_todos(&self.pool, include_done, i64::from(limit)).await?)
    }

    /// 完成待办（幂等：已完成/不存在 → false 不发事件；完成 → TodoCompleted → gamify Xp(1)，docs/12 complete）
    pub async fn todo_complete(&self, todo_id: &str) -> Result<bool, EngineError> {
        let Some(todo) = ambition_store::get_todo(&self.pool, todo_id).await? else {
            return Ok(false);
        };
        if todo.status == TodoStatus::Done {
            return Ok(false);
        }
        let title = todo.title.clone();
        if ambition_store::todo_complete(&self.pool, todo_id).await? {
            let _ = self
                .settle(ambition_core::DomainEvent::TodoCompleted {
                    todo_id: todo_id.to_string(),
                    title,
                })
                .await;
            return Ok(true);
        }
        Ok(false)
    }

    /// 考前排期（docs/12 plan_for_exam）：未排期（due=None）待办均摊到 (今天, 考试日前) 开区间；
    /// 已有 due 保留；考试日非法/不晚于今天 → NotFound。返回重排后的未完成清单。
    pub async fn plan_for_exam(&self, exam_date: &str) -> Result<Vec<Todo>, EngineError> {
        let exam = date_to_unix(exam_date)
            .ok_or_else(|| EngineError::NotFound(format!("非法日期：{exam_date}")))?;
        let today = date_to_unix(&today()).unwrap_or(0);
        if exam <= today {
            return Err(EngineError::NotFound(format!(
                "考试日须晚于今天：{exam_date}"
            )));
        }
        let todos = ambition_store::list_todos(&self.pool, false, 500).await?;
        let undated: Vec<usize> = todos
            .iter()
            .enumerate()
            .filter(|(_, t)| t.due.is_none())
            .map(|(i, _)| i)
            .collect();
        let n = undated.len() as i64;
        if n == 0 {
            return Ok(todos);
        }
        let span = exam - today;
        for (k, i) in undated.iter().enumerate() {
            // 均摊到开区间 (today, exam)——考试日当天留冲刺不排新
            let due = today + span * (k as i64 + 1) / (n + 1);
            ambition_store::update_todo_due(&self.pool, &todos[*i].todo_id.0, due).await?;
        }
        Ok(ambition_store::list_todos(&self.pool, false, 500).await?)
    }

    /// 每日清单（docs/12 daily_checklist）：due ≤ 今日末 的未完成待办（null-due 未排期不进）
    pub async fn daily_checklist(&self) -> Result<Vec<Todo>, EngineError> {
        let today_end = date_to_unix(&today()).unwrap_or(0) + 86_400;
        let todos = ambition_store::list_todos(&self.pool, false, 500).await?;
        Ok(todos
            .into_iter()
            .filter(|t| t.due.is_some_and(|d| d < today_end))
            .collect())
    }

    /// 生词批量入笔记（按 title+word 去重；Word 骨架笔记随 46-vocab 词库波次补全，docs/12 link_new_words）
    pub async fn link_new_words(&self, words: &[String]) -> Result<u32, EngineError> {
        let mut added = 0u32;
        for word in words {
            let title = word.trim();
            if title.is_empty() {
                continue;
            }
            if ambition_store::find_note_by_title(&self.pool, title, "word")
                .await?
                .is_some()
            {
                continue;
            }
            let now = ambition_core::now_unix();
            let note = Note {
                note_id: ambition_core::NoteId::new_v7(),
                kind: NoteKind::Word,
                title: title.to_string(),
                body_md: format!("# {title}\n"),
                links: vec![],
                created_at: now,
                updated_at: now,
            };
            ambition_store::upsert_note(&self.pool, &note).await?;
            added += 1;
        }
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn engine() -> Engine {
        Engine::open_memory().await.expect("开内存库")
    }

    #[tokio::test]
    async fn test_todo_add_then_list_shows_pending() {
        let engine = engine().await;
        let todo = engine
            .todo_add(TodoKind::ExamPrep, "模考错题二刷".into(), None)
            .await
            .expect("加");
        assert_eq!(todo.status, TodoStatus::Pending);
        let all = engine.todo_list(false, 10).await.expect("列");
        assert_eq!(all.len(), 1);
        assert!(all[0].due.is_none(), "未排期 due 空");
    }

    #[tokio::test]
    async fn test_todo_list_filters_done_when_excluded() {
        let engine = engine().await;
        let todo = engine
            .todo_add(TodoKind::ExamPrep, "t".into(), None)
            .await
            .expect("加");
        assert!(engine.todo_complete(&todo.todo_id.0).await.expect("完成"));
        let open = engine.todo_list(false, 10).await.expect("列");
        assert!(open.is_empty(), "完成被过滤");
        let all = engine.todo_list(true, 10).await.expect("列");
        assert_eq!(all.len(), 1, "含完成");
    }

    #[tokio::test]
    async fn test_todo_complete_is_idempotent_and_publishes_event() {
        let engine = engine().await;
        let todo = engine
            .todo_add(TodoKind::ExamPrep, "写周报错题".into(), None)
            .await
            .expect("加");
        assert!(engine.todo_complete(&todo.todo_id.0).await.expect("完成"));
        assert!(
            !engine.todo_complete(&todo.todo_id.0).await.expect("再完成"),
            "幂等 false"
        );
        assert!(
            !engine.todo_complete("missing").await.expect("不存在"),
            "不存在 false"
        );
        let events = engine.drain_events();
        let hits: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ambition_core::DomainEvent::TodoCompleted { .. }))
            .collect();
        assert_eq!(hits.len(), 1, "只发一次事件");
    }

    #[tokio::test]
    async fn test_plan_for_exam_spreads_undated_and_keeps_dated() {
        let engine = engine().await;
        let a = engine
            .todo_add(TodoKind::ExamPrep, "a".into(), None)
            .await
            .expect("a");
        let b = engine
            .todo_add(TodoKind::ExamPrep, "b".into(), None)
            .await
            .expect("b");
        let dated = engine
            .todo_add(TodoKind::ExamPrep, "dated".into(), Some(1_000))
            .await
            .expect("dated");
        let exam = unix_to_date(date_to_unix(&today()).expect("今日可解析") + 30 * 86_400);
        let planned = engine.plan_for_exam(&exam).await.expect("排期");
        assert_eq!(planned.len(), 3);
        let open_a = planned.iter().find(|t| t.todo_id == a.todo_id).expect("a");
        let open_b = planned.iter().find(|t| t.todo_id == b.todo_id).expect("b");
        let got_dated = planned
            .iter()
            .find(|t| t.todo_id == dated.todo_id)
            .expect("dated");
        assert!(open_a.due.is_some() && open_b.due.is_some(), "未排期被排上");
        assert_eq!(got_dated.due, Some(1_000), "已有 due 保留");
        assert_ne!(open_a.due, open_b.due, "均摊不重叠");
        // 非法考试日
        assert!(
            engine.plan_for_exam("not-a-date").await.is_err(),
            "非法日拒"
        );
    }

    #[tokio::test]
    async fn test_daily_checklist_filters_by_today_end() {
        let engine = engine().await;
        let today_end = date_to_unix(&today()).expect("今日可解析") + 86_400;
        engine
            .todo_add(TodoKind::ExamPrep, "今天".into(), Some(today_end - 60))
            .await
            .expect("今天");
        engine
            .todo_add(
                TodoKind::ExamPrep,
                "远期".into(),
                Some(today_end + 7 * 86_400),
            )
            .await
            .expect("远期");
        engine
            .todo_add(TodoKind::ExamPrep, "未排期".into(), None)
            .await
            .expect("未排");
        let list = engine.daily_checklist().await.expect("清单");
        assert_eq!(list.len(), 1, "只有今日到期/逾期在清单");
        assert_eq!(list[0].title, "今天");
    }

    #[tokio::test]
    async fn test_link_new_words_dedupes_by_title() {
        let engine = engine().await;
        let added = engine
            .link_new_words(&["take".into(), " make ".into(), "take".into(), "  ".into()])
            .await
            .expect("入库");
        assert_eq!(added, 2, "去重+空白剔除");
        let again = engine.link_new_words(&["take".into()]).await.expect("重复");
        assert_eq!(again, 0, "已存在不重");
        let hits = engine.note_search("take", 10).await.expect("检索");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, NoteKind::Word, "Word 桶");
    }
}
