//! 笔记 / 日记 / TODO / 计划类型（M6-W1，docs/12 §1 定案）
//!
//! 行类型放 core：store 落库、engine 门面、TUI 三面共用（同 Translation/Paper 先例）。
//! ID 注册表（docs/02 §1）：NoteId/EntryId/TodoId = uuid v7（TEXT PK）；
//! LinkRef::Word = lemma 字符串（稳定 word_id 随 09 EC-DICT 导入）。

use super::question::EssayPrompt;
use crate::{CardId, Qid, now_unix};

/// uuid v7 新类型宏（时间有序；注册表 docs/02 §1——三个 ID 共用实现，配对测试一次覆盖）
macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub struct $name(pub String);

        impl $name {
            /// uuid v7（毫秒时间戳前缀，单调递增——列表默认序 = 创建序）
            pub fn new_v7() -> Self {
                Self(uuid::Uuid::now_v7().to_string())
            }
        }
    };
}

uuid_id!(NoteId);
uuid_id!(EntryId);
uuid_id!(TodoId);

/// 笔记类型（词/题/语法点/句式/自由——五桶，docs/12 §1）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    Word,
    Question,
    Grammar,
    Phrase,
    Free,
}

/// 双链引用（词↔题↔语法点↔作文↔笔记；Word = lemma——稳定 word_id 随 09 导入）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LinkRef {
    Word(String),
    Qid(Qid),
    Grammar(String),
    Essay(String),
    Note(NoteId),
}

/// 笔记（纯 Markdown 正文 + 双链；导出 front-matter 见 engine::notes，`~/.ambition/notes/*.md`）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Note {
    pub note_id: NoteId,
    pub kind: NoteKind,
    pub title: String,
    pub body_md: String,
    pub links: Vec<LinkRef>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Note {
    /// 新笔记（id=uuid v7；链接由 link_new_words/关联动作补写）
    pub fn new(title: String, body_md: String, kind: NoteKind) -> Self {
        let now = now_unix();
        Self {
            note_id: NoteId::new_v7(),
            kind,
            title,
            body_md,
            links: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// 日记批改产物（review_diary：只改错 + ≤2 升级 + 一句鼓励；仅请求批改时外发——docs/12 §1 隐私红线）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiaryReview {
    pub corrected_md: String,
    pub upgrades: Vec<String>,
    pub encouragement: String,
    pub model: String,
    pub reviewed_at: i64,
}

/// 日记条目（date 唯一——一日一篇；批改可选）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiaryEntry {
    pub entry_id: EntryId,
    pub date: String,
    pub body: String,
    pub review: Option<DiaryReview>,
}

impl DiaryEntry {
    /// 新日记（date = `YYYY-MM-DD`，同日 upsert 由 store 按 UNIQUE 收口）
    pub fn new(date: String, body: String) -> Self {
        Self {
            entry_id: EntryId::new_v7(),
            date,
            body,
            review: None,
        }
    }
}

/// 每日预算（16-config step_budget 载荷；interleaved_plan/follow 消费）
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DailyBudget {
    pub minutes: u16,
    pub new_words: u16,
    pub reviews: u16,
    pub drills: u16,
}

/// 计划条目（kind：review/new/drill/writing——目标数 + 已完成数）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanItem {
    pub kind: String,
    pub title: String,
    pub target: u16,
    pub done: u16,
}

/// 每日清单（plan_for_exam / interleaved_plan / daily_checklist 同源共用——docs/16 §2）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DailyPlan {
    pub date: String,
    pub items: Vec<PlanItem>,
}

/// 待办类型（Daily 计划任务 / Review 复习卡 / Drill 专项 / Writing 写作 / ExamPrep 考前——docs/12 §1）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TodoKind {
    Daily(DailyPlan),
    Review(CardId),
    Drill(String),
    Writing(EssayPrompt),
    ExamPrep,
}

/// 待办状态（pending|done——store 列为 snake_case 文本）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    Done,
}

/// 待办（due = unix 秒可空；meta = 扩展 JSON 字符串）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Todo {
    pub todo_id: TodoId,
    pub kind: TodoKind,
    pub title: String,
    pub due: Option<i64>,
    pub status: TodoStatus,
    pub meta: String,
    pub created_at: i64,
}

impl Todo {
    /// 新待办（status=Pending；meta 空 JSON 对象）
    pub fn new(kind: TodoKind, title: String, due: Option<i64>) -> Self {
        Self {
            todo_id: TodoId::new_v7(),
            kind,
            title,
            due,
            status: TodoStatus::Pending,
            meta: "{}".to_string(),
            created_at: now_unix(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_v7_is_time_ordered_and_hyphenated() {
        let a = NoteId::new_v7();
        let b = EntryId::new_v7();
        assert_eq!(a.0.len(), 36, "uuid 连字符 36 字符");
        assert_eq!(a.0.as_bytes()[14], b'7', "uuid v7 版本位（第14位=7）");
        assert_eq!(TodoId::new_v7().0.len(), 36);
        // 毫秒前缀非回退（同毫秒内的随机位不保证字典序——v7 单调只到毫秒粒度）
        let ms = |id: &String| u64::from_str_radix(&id[0..13], 16).unwrap_or(0);
        assert!(ms(&b.0) <= ms(&a.0), "时间前缀不回退");
    }

    #[test]
    fn test_note_new_fills_id_and_timestamps() {
        let note = Note::new("完成时".into(), "##要点\n- 例句".into(), NoteKind::Word);
        assert_eq!(note.note_id.0.len(), 36);
        assert_eq!(note.created_at, note.updated_at);
        assert!(note.links.is_empty(), "新笔记无链接");
        assert_eq!(note.kind, NoteKind::Word);
    }

    #[test]
    fn test_diary_entry_new_is_unreviewed_for_date() {
        let entry = DiaryEntry::new("2026-09-29".into(), "今天复盘…".into());
        assert_eq!(entry.date, "2026-09-29");
        assert!(entry.review.is_none(), "未批改");
        assert_eq!(entry.entry_id.0.len(), 36);
    }

    #[test]
    fn test_todo_new_is_pending_with_empty_meta() {
        let todo = Todo::new(
            TodoKind::ExamPrep,
            "模考 20 天倒计时".into(),
            Some(1_800_000_000),
        );
        assert_eq!(todo.status, TodoStatus::Pending);
        assert_eq!(todo.meta, "{}");
        assert_eq!(todo.due, Some(1_800_000_000));
        assert_eq!(todo.todo_id.0.len(), 36);
    }

    #[test]
    fn test_todo_kind_roundtrips_daily_plan_and_essay_prompt() {
        let plan = DailyPlan {
            date: "2026-09-29".into(),
            items: vec![PlanItem {
                kind: "review".into(),
                title: "错词 10".into(),
                target: 10,
                done: 0,
            }],
        };
        let todo = Todo::new(TodoKind::Daily(plan), "今日清单".into(), None);
        let json = serde_json::to_string(&todo).expect("序列化");
        let back: Todo = serde_json::from_str(&json).expect("反序列化");
        assert_eq!(back.kind, todo.kind, "DailyPlan 内嵌往返");
        let w = Todo::new(
            TodoKind::Writing(EssayPrompt {
                prompt_zh: "题面".into(),
                genre: "argument".into(),
                track: "cet4".into(),
                rubric_id: "gaokao25".into(),
                word_range: (120, 180),
                minutes: 30,
            }),
            "写作".into(),
            None,
        );
        let json = serde_json::to_string(&w).expect("序列化");
        let _: Todo = serde_json::from_str(&json).expect("反序列化");
        // LinkRef 外置标签往返（双链 payload 稳定性）
        let link = LinkRef::Qid(Qid("q1".into()));
        let json = serde_json::to_string(&link).expect("序列化");
        let back: LinkRef = serde_json::from_str(&json).expect("反序列化");
        assert_eq!(back, link);
    }
}
