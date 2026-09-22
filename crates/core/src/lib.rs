//! Ambition core：领域类型与纯函数（M1 骨架，设计见 docs/02-core-store）

use sha2::{Digest, Sha256};

/// 内容寻址题目 ID：sha256(题目内容) 前 8 字节 hex（16 字符）
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Qid(pub String);

impl Qid {
    /// 由题目内容生成稳定 id（同内容同 id，用于去重与共享）
    pub fn from_content(content: &str) -> Self {
        let digest = Sha256::digest(content.as_bytes());
        let hex: String = digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Self(hex)
    }
}

/// BCP-47 语言标签（"en" / "zh-Hans" …，docs/16-config-i18n）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LangTag(pub String);

/// 错因标签（画像原料，docs/07-grading §归类；M1 先行子集）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ErrorTag {
    WordSense,
    Collocation,
    Tense,
    Spelling,
    Article,
    Careless,
    NoError,
    Other,
}

impl ErrorTag {
    /// 稳定字符串（snake_case，入库与 Jev criteria 共用口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WordSense => "word_sense",
            Self::Collocation => "collocation",
            Self::Tense => "tense",
            Self::Spelling => "spelling",
            Self::Article => "article",
            Self::Careless => "careless",
            Self::NoError => "no_error",
            Self::Other => "other",
        }
    }
}

/// 判定结果（Pending = 待裁定桶，docs/07-grading）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Verdict {
    Correct,
    Partial,
    Wrong,
    Acceptable,
    Pending,
}

/// 判分来源（M1 只有 Rule；Jev/LLM 为 M3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GradedBy {
    Rule,
    Jev,
    LlmReview,
    Human,
}

/// 作答场景
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AttemptMode {
    Practice,
    Review,
    Mock,
    Game,
    Typing,
}

/// 选项
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OptionItem {
    pub id: u32,
    pub text: String,
}

/// 选择题体（单选 answer 只有一个下标；多选可多个）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChoiceBody {
    pub options: Vec<OptionItem>,
    pub answer: Vec<u32>,
}

/// 填空空位（可接受答案表，规则层直接比对）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlankItem {
    pub acceptable: Vec<String>,
}

/// 填空题体
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FillBody {
    pub sentence: String,
    pub blanks: Vec<BlankItem>,
}

/// M1 支持的题型（补全题型见 docs/06-generation）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QuestionKind {
    SingleChoice(ChoiceBody),
    MultiChoice(ChoiceBody),
    FillBlank(FillBody),
}

/// 题目（内容寻址 qid）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Question {
    pub qid: Qid,
    pub kind: QuestionKind,
    pub stem: String,
    pub difficulty: u8,
    pub tags: Vec<String>,
}

impl Question {
    /// 构造题目并计算内容寻址 qid（同内容同 id）
    pub fn new(stem: String, kind: QuestionKind, difficulty: u8, tags: Vec<String>) -> Self {
        let content =
            serde_json::to_string(&(&stem, &kind, &difficulty, &tags)).unwrap_or_default();
        let qid = Qid::from_content(&content);
        Self {
            qid,
            kind,
            stem,
            difficulty,
            tags,
        }
    }
}

/// 作答（M1：选择 / 填空）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Answer {
    Choice(Vec<u32>),
    Fill(Vec<String>),
}

/// 计时信号（假对检测输入）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Timing {
    pub total_ms: u64,
    pub revision_count: u32,
}

/// 用户延迟基线（近 200 次滚动分位数，docs/08-adaptive §假对）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UserBaseline {
    pub latency_p50_ms: u64,
    pub latency_p90_ms: u64,
}

/// 一次作答
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Attempt {
    pub attempt_id: String,
    pub qid: Qid,
    pub answer: Answer,
    pub timing: Option<Timing>,
    pub self_confidence: Option<u8>,
    pub mode: AttemptMode,
}

/// 逐项判定（多选/完形）
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ItemResult {
    pub index: usize,
    pub correct: bool,
}

/// 判分结果（M1 规则层）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GradeResult {
    pub verdict: Verdict,
    pub score: f32,
    pub max: f32,
    pub confidence: f32,
    pub graded_by: GradedBy,
    pub items: Vec<ItemResult>,
    pub error_tags: Vec<ErrorTag>,
}

/// 领域事件（gamify/sync/profile 订阅，docs/02-core-store §DomainEvent）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DomainEvent {
    AttemptGraded {
        attempt_id: String,
        qid: Qid,
        verdict: Verdict,
        correct: bool,
    },
}

/// 规范化比对：小写、全角转半角、折叠空白、去尾部标点
pub fn normalize_text(input: &str) -> String {
    let mapped: String = input
        .chars()
        .map(|c| match c {
            '\u{3000}' => ' ',
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .collect();
    let lowered = mapped.to_lowercase();
    lowered
        .trim()
        .trim_end_matches(|c: char| ".,!?;:".contains(c))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 假对判定（猜对 ≠ 会做，docs/08-adaptive §假对三形态）
pub fn is_guess(timing: &Timing, self_confidence: Option<u8>, baseline: &UserBaseline) -> bool {
    if self_confidence == Some(1) {
        return true;
    }
    if timing.revision_count == 0
        && u128::from(timing.total_ms) < u128::from(baseline.latency_p50_ms) * 3 / 10
    {
        return true;
    }
    self_confidence.is_some_and(|c| c <= 2) && timing.total_ms > baseline.latency_p90_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_choice_body() -> ChoiceBody {
        ChoiceBody {
            options: vec![
                OptionItem {
                    id: 0,
                    text: "quick".into(),
                },
                OptionItem {
                    id: 1,
                    text: "slowly".into(),
                },
            ],
            answer: vec![0],
        }
    }

    #[test]
    fn test_from_content_is_stable_and_distinct() {
        assert_eq!(Qid::from_content("abc"), Qid::from_content("abc"));
        assert_eq!(Qid::from_content("abc").0.len(), 16);
        assert_ne!(Qid::from_content("abc"), Qid::from_content("abd"));
    }

    #[test]
    fn test_as_str_is_snake_case() {
        assert_eq!(ErrorTag::WordSense.as_str(), "word_sense");
        assert_eq!(ErrorTag::NoError.as_str(), "no_error");
        assert_eq!(ErrorTag::Careless.as_str(), "careless");
    }

    #[test]
    fn test_new_computes_stable_qid() {
        let a = Question::new(
            "She is ___ student.".into(),
            QuestionKind::FillBlank(FillBody {
                sentence: "She is ___ student.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["a".into()],
                }],
            }),
            2,
            vec!["article".into()],
        );
        let b = Question::new(a.stem.clone(), a.kind.clone(), 2, vec!["article".into()]);
        assert_eq!(a.qid, b.qid);
        let c = Question::new(a.stem.clone(), a.kind.clone(), 4, vec!["article".into()]);
        assert_ne!(a.qid, c.qid, "难度参与内容哈希");
    }

    #[test]
    fn test_normalize_text_lowercases_and_strips() {
        assert_eq!(normalize_text("  The Student,  "), "the student");
        assert_eq!(normalize_text("Ａ Student!"), "a student"); // 全角转半角
        assert_eq!(normalize_text("She\tis\nok."), "she is ok");
        assert_eq!(normalize_text(""), "");
    }

    #[test]
    fn test_is_guess_covers_three_forms() {
        let base = UserBaseline {
            latency_p50_ms: 10_000,
            latency_p90_ms: 30_000,
        };
        // 形态①：秒选（< p50×0.3 且零修订）
        let quick = Timing {
            total_ms: 1_000,
            revision_count: 0,
        };
        assert!(is_guess(&quick, Some(4), &base));
        // 形态②：久犹豫低自评（> p90 且 conf ≤ 2）
        let slow = Timing {
            total_ms: 35_000,
            revision_count: 5,
        };
        assert!(is_guess(&slow, Some(2), &base));
        // 形态③：自评完全没把握
        let mid = Timing {
            total_ms: 15_000,
            revision_count: 1,
        };
        assert!(is_guess(&mid, Some(1), &base));
        // 正常作答不是假对
        let normal = Timing {
            total_ms: 12_000,
            revision_count: 2,
        };
        assert!(!is_guess(&normal, Some(4), &base));
    }

    #[test]
    fn test_question_kind_holds_choice_body() {
        let body = test_choice_body();
        let q = Question::new(
            "choose".into(),
            QuestionKind::SingleChoice(body),
            1,
            vec!["vocab".into()],
        );
        assert_eq!(q.stem, "choose");
        assert_eq!(q.difficulty, 1);
    }
}
