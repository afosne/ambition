//! 待办仓（M6-W1，docs/02 §2 TodoRepo）：列（due 升序）+ 完成（幂等返回是否有行变更）
use ambition_core::{Todo, TodoStatus};

use crate::*;

type TodoRow = (String, String, String, Option<i64>, String, String, i64);

/// 状态列值（status 列为裸文本：pending/done——查询可直接比较）
fn status_str(status: &TodoStatus) -> &'static str {
    match status {
        TodoStatus::Pending => "pending",
        TodoStatus::Done => "done",
    }
}

/// 状态列反解（未知值 → Pending 防御，不静默丢行）
fn status_from(raw: &str) -> TodoStatus {
    match raw {
        "done" => TodoStatus::Done,
        _ => TodoStatus::Pending,
    }
}

/// 加待办（todo_id = core uuid v7；kind 列为 TodoKind JSON 供过滤、整行以列还原）
pub async fn add_todo(pool: &SqlitePool, todo: &Todo) -> Result<(), sqlx::Error> {
    let kind = serde_json::to_string(&todo.kind)
        .map_err(|err| sqlx::Error::Decode(format!("todo kind encode: {err}").into()))?;
    let meta = serde_json::to_string(&todo.meta)
        .map_err(|err| sqlx::Error::Decode(format!("todo meta encode: {err}").into()))?;
    sqlx::query(
        "INSERT INTO todos (todo_id, kind, title, due, status, meta, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&todo.todo_id.0)
    .bind(kind)
    .bind(&todo.title)
    .bind(todo.due)
    .bind(status_str(&todo.status))
    .bind(meta)
    .bind(todo.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// 按 ID 读取单条待办（完成事件读取标题用；不受列表分页限制）
pub async fn get_todo(pool: &SqlitePool, todo_id: &str) -> Result<Option<Todo>, sqlx::Error> {
    let row: Option<TodoRow> = sqlx::query_as(
        "SELECT todo_id, kind, title, due, status, meta, created_at FROM todos WHERE todo_id = ?",
    )
    .bind(todo_id)
    .fetch_optional(pool)
    .await?;
    row.map(decode_todo).transpose()
}

fn decode_todo(
    (todo_id, kind, title, due, status, meta, created_at): TodoRow,
) -> Result<Todo, sqlx::Error> {
    let kind = serde_json::from_str(&kind)
        .map_err(|err| sqlx::Error::Decode(format!("todo kind 损坏（{todo_id}）: {err}").into()))?;
    let meta = serde_json::from_str(&meta)
        .map_err(|err| sqlx::Error::Decode(format!("todo meta 损坏（{todo_id}）: {err}").into()))?;
    Ok(Todo {
        todo_id: ambition_core::TodoId(todo_id),
        kind,
        title,
        due,
        status: status_from(&status),
        meta,
        created_at,
    })
}

/// 列表（due 升序、无日期垫底；include_done=false = 只看未完成——daily_checklist 复用）
pub async fn list_todos(
    pool: &SqlitePool,
    include_done: bool,
    limit: i64,
) -> Result<Vec<Todo>, sqlx::Error> {
    let sql = if include_done {
        "SELECT todo_id, kind, title, due, status, meta, created_at FROM todos
         ORDER BY (due IS NULL), due, created_at LIMIT ?"
    } else {
        "SELECT todo_id, kind, title, due, status, meta, created_at FROM todos
         WHERE status != 'done'
         ORDER BY (due IS NULL), due, created_at LIMIT ?"
    };
    let rows: Vec<TodoRow> = sqlx::query_as(sql).bind(limit).fetch_all(pool).await?;
    rows.into_iter().map(decode_todo).collect()
}

/// 完成待办（pending→done；幂等——重复完成返回 false，引擎据此决定是否发事件）
pub async fn todo_complete(pool: &SqlitePool, todo_id: &str) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("UPDATE todos SET status = 'done' WHERE todo_id = ? AND status != 'done'")
            .bind(todo_id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// 改期（plan_for_exam 排期回写；单行 UPDATE 幂等）
pub async fn update_todo_due(
    pool: &SqlitePool,
    todo_id: &str,
    due: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE todos SET due = ? WHERE todo_id = ?")
        .bind(due)
        .bind(todo_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use ambition_core::{Todo, TodoKind, TodoStatus};

    use super::*;
    use crate::common::fixtures::memory_pool;

    fn todo(title: &str) -> Todo {
        Todo {
            todo_id: ambition_core::TodoId::new_v7(),
            kind: TodoKind::Drill("mistake_verb".to_string()),
            title: title.to_string(),
            due: Some(1_700_000_000),
            status: TodoStatus::Pending,
            meta: serde_json::json!({ "count": 10 }).to_string(),
            created_at: 1_700_000_000,
        }
    }

    #[tokio::test]
    async fn test_add_todo_persists_row_with_kind_and_status() {
        let pool = memory_pool().await;
        let t = todo("新建待办");
        add_todo(&pool, &t).await.expect("写入");
        let got = get_todo(&pool, &t.todo_id.0).await.unwrap().expect("存在");
        assert_eq!(got.title, "新建待办");
        assert_eq!(got.status, TodoStatus::Pending);
    }

    #[test]
    fn test_status_str_roundtrips_pending_and_done() {
        assert_eq!(status_str(&TodoStatus::Pending), "pending");
        assert_eq!(status_str(&TodoStatus::Done), "done");
        assert_eq!(status_from("pending"), TodoStatus::Pending);
        assert_eq!(status_from("done"), TodoStatus::Done);
    }

    #[test]
    fn test_status_from_defaults_unknown_to_pending() {
        assert_eq!(status_from("?"), TodoStatus::Pending, "未知值防御不丢行");
    }

    #[tokio::test]
    async fn test_list_todos_orders_due_and_filters_done() {
        let pool = memory_pool().await;
        let mut t1 = todo("复习错因本");
        t1.due = Some(1_700_000_100);
        let mut t2 = todo("已完成项");
        t2.status = TodoStatus::Done;
        add_todo(&pool, &t1).await.expect("加1");
        add_todo(&pool, &t2).await.expect("加2");
        let open = list_todos(&pool, false, 10).await.expect("未完成列");
        assert_eq!(open.len(), 1, "done 默认过滤");
        assert_eq!(open[0].title, "复习错因本");
        let meta: serde_json::Value = serde_json::from_str(&open[0].meta).expect("meta JSON");
        assert_eq!(meta["count"], 10, "meta JSON 往返");
        let all = list_todos(&pool, true, 10).await.expect("全列");
        assert_eq!(all.len(), 2, "include_done=true");
        assert!(all[0].due <= all[1].due || all[0].due.is_none(), "due 升序");
    }

    #[tokio::test]
    async fn test_get_todo_roundtrips_single_row() {
        let pool = memory_pool().await;
        let value = todo("读文章");
        add_todo(&pool, &value).await.expect("新增");
        assert_eq!(
            get_todo(&pool, &value.todo_id.0).await.unwrap(),
            Some(value)
        );
        assert_eq!(get_todo(&pool, "missing").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_decode_todo_rejects_invalid_kind_payload() {
        let error = decode_todo((
            "bad".into(),
            "{".into(),
            "t".into(),
            None,
            "pending".into(),
            "{}".into(),
            0,
        ))
        .expect_err("坏 JSON 应显式报错");
        assert!(matches!(error, sqlx::Error::Decode(_)));
    }

    #[tokio::test]
    async fn test_update_todo_due_reschedules_open_row() {
        let pool = memory_pool().await;
        let t = todo("排期项");
        add_todo(&pool, &t).await.expect("加");
        assert!(
            update_todo_due(&pool, &t.todo_id.0, 99)
                .await
                .expect("改期"),
            "命中一行"
        );
        let rows = list_todos(&pool, true, 10).await.expect("列");
        assert_eq!(rows[0].due, Some(99), "due 已回写");
        assert!(
            !update_todo_due(&pool, "missing", 1).await.expect("未知"),
            "未知 id 无行"
        );
    }

    #[tokio::test]
    async fn test_todo_complete_flips_status_once_then_idempotent() {
        let pool = memory_pool().await;
        let t = todo("一次性");
        add_todo(&pool, &t).await.expect("加");
        assert!(
            todo_complete(&pool, &t.todo_id.0).await.expect("完成"),
            "首完成 true"
        );
        assert!(
            !todo_complete(&pool, &t.todo_id.0).await.expect("再完成"),
            "幂等 false"
        );
        let rows = list_todos(&pool, true, 10).await.expect("列");
        assert_eq!(rows[0].status, TodoStatus::Done, "状态落库");
        assert!(
            !todo_complete(&pool, "missing").await.expect("未知"),
            "未知 id 无行"
        );
    }
}
