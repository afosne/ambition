//! Ambition core：领域类型与纯函数（M1 骨架 + M2 出题/质检类型，设计见 docs/02-core-store）

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

impl LangTag {
    /// 默认目标语 en（M1 单语默认，多语种随 M9）
    pub fn english() -> Self {
        Self("en".into())
    }
}

impl Default for LangTag {
    /// 默认 en（非空串：默认值直接可用）
    fn default() -> Self {
        Self::english()
    }
}

/// 内容哈希（sha256 hex，qpack 防篡改 / 缓存 key，docs/02-core-store §1）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContentHash(pub String);

/// 规范 JSON 内容哈希：同一内容（含字段序）恒得同一哈希
pub fn canonical_json_hash<T: serde::Serialize>(value: &T) -> ContentHash {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    ContentHash(hex)
}

/// 题目出处（seed 内置 / LLM 生成 / qpack 导入 / 判题沉淀，docs/06-generation §7）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// 内置 seed 题库
    #[default]
    Seed,
    /// LLM 结构化出题（06）
    Generated,
    /// qpack 导入（他人分享）
    Imported,
    /// 判题/讲解自动沉淀
    Collected,
}

impl Provenance {
    /// 持久化字符串（snake_case，与 serde 同口径 —— 漂移由 test_as_str 匹配 serde 断言）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Seed => "seed",
            Self::Generated => "generated",
            Self::Imported => "imported",
            Self::Collected => "collected",
        }
    }
}

/// 错因标签（画像原料，docs/07-grading §归类；M1 先行子集）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Correct,
    Partial,
    Wrong,
    Acceptable,
    Pending,
}

impl Verdict {
    /// 持久化字符串（snake_case，与 serde 同口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Correct => "correct",
            Self::Partial => "partial",
            Self::Wrong => "wrong",
            Self::Acceptable => "acceptable",
            Self::Pending => "pending",
        }
    }
}

/// 判分来源（M1 只有 Rule；Jev/LLM 为 M3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradedBy {
    Rule,
    Jev,
    LlmReview,
    Human,
}

impl GradedBy {
    /// 持久化字符串（snake_case，与 serde 同口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Jev => "jev",
            Self::LlmReview => "llm_review",
            Self::Human => "human",
        }
    }
}

/// 作答场景
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptMode {
    Practice,
    Review,
    Mock,
    Game,
    Typing,
}

impl AttemptMode {
    /// 持久化字符串（snake_case，与 serde 同口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Practice => "practice",
            Self::Review => "review",
            Self::Mock => "mock",
            Self::Game => "game",
            Self::Typing => "typing",
        }
    }
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
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    SingleChoice(ChoiceBody),
    MultiChoice(ChoiceBody),
    FillBlank(FillBody),
}

/// 题目内容（参与 qid 寻址的字段集 —— 具名结构，schema 演化不致串位，docs/02-core-store §2）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuestionContent {
    pub stem: String,
    pub kind: QuestionKind,
    pub difficulty: u8,
    pub tags: Vec<String>,
}

/// 题目（内容寻址 qid；lang/provenance/quality_score 不参与寻址）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Question {
    pub qid: Qid,
    pub kind: QuestionKind,
    pub stem: String,
    pub difficulty: u8,
    pub tags: Vec<String>,
    pub lang: LangTag,
    pub provenance: Provenance,
    pub quality_score: f32,
}

impl Question {
    /// 构造题目并计算内容寻址 qid（同内容同 id；lang/provenance/quality 不参与寻址）
    pub fn new(
        stem: String,
        kind: QuestionKind,
        difficulty: u8,
        tags: Vec<String>,
        provenance: Provenance,
    ) -> Self {
        let content = QuestionContent {
            stem,
            kind,
            difficulty,
            tags,
        };
        let qid = Qid::from_content(&serde_json::to_string(&content).unwrap_or_default());
        let QuestionContent {
            stem,
            kind,
            difficulty,
            tags,
        } = content;
        Self {
            qid,
            kind,
            stem,
            difficulty,
            tags,
            lang: LangTag::english(),
            provenance,
            quality_score: 1.0,
        }
    }

    /// 由内容重建题目（qid 重算，provenance 显式指定；导入/生成路径用，P-4 显式逻辑）
    pub fn with_content(content: QuestionContent, provenance: Provenance) -> Self {
        let mut q = Self::new(
            content.stem,
            content.kind,
            content.difficulty,
            content.tags,
            provenance,
        );
        q.provenance = provenance;
        q
    }

    /// 参与寻址的内容视图（存库 payload / qid 校验）
    pub fn content(&self) -> QuestionContent {
        QuestionContent {
            stem: self.stem.clone(),
            kind: self.kind.clone(),
            difficulty: self.difficulty,
            tags: self.tags.clone(),
        }
    }
}

/// 作答（M1：选择 / 填空）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub enum DomainEvent {
    AttemptGraded {
        attempt_id: String,
        qid: Qid,
        verdict: Verdict,
        correct: bool,
    },
}

/// 事件总线：订阅者（gamify/sync 投影/profile）旁路消费，各自幂等；engine 只 publish 不直调（改进 #3）
pub trait EventBus: Send + Sync {
    /// 消费一条不可变事实
    fn publish(&self, ev: DomainEvent);
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
            Provenance::Seed,
        );
        let b = Question::new(
            a.stem.clone(),
            a.kind.clone(),
            2,
            vec!["article".into()],
            Provenance::Seed,
        );
        assert_eq!(a.qid, b.qid);
        let c = Question::new(
            a.stem.clone(),
            a.kind.clone(),
            4,
            vec!["article".into()],
            Provenance::Seed,
        );
        assert_ne!(a.qid, c.qid, "难度参与内容哈希");
        // provenance/lang/quality 不参与寻址：同一内容不同出处同 qid
        let d = Question::new(
            a.stem.clone(),
            a.kind.clone(),
            2,
            vec!["article".into()],
            Provenance::Generated,
        );
        assert_eq!(a.qid, d.qid, "provenance 不参与内容寻址");
        assert_eq!(a.lang, LangTag::english());
        assert_eq!(a.quality_score, 1.0);
    }

    #[test]
    fn test_with_content_rebuilds_and_content_roundtrips() {
        let original = Question::new(
            "Pick one.".into(),
            QuestionKind::SingleChoice(test_choice_body()),
            3,
            vec!["vocab".into()],
            Provenance::Seed,
        );
        let view = original.content();
        assert_eq!(view.stem, original.stem);
        assert_eq!(view.difficulty, 3);
        let rebuilt = Question::with_content(view, Provenance::Imported);
        assert_eq!(rebuilt.qid, original.qid, "同内容重算 qid 稳定");
        assert_eq!(rebuilt.provenance, Provenance::Imported);
    }

    #[test]
    fn test_content_view_matches_addressed_fields() {
        let original = Question::new(
            "Stem.".into(),
            QuestionKind::SingleChoice(test_choice_body()),
            5,
            vec!["t1".into(), "t2".into()],
            Provenance::Seed,
        );
        let view = original.content();
        assert_eq!(view.stem, "Stem.");
        assert_eq!(view.difficulty, 5);
        assert_eq!(view.tags, vec!["t1".to_string(), "t2".to_string()]);
        assert_eq!(view.kind, original.kind);
    }

    #[test]
    fn test_default_lang_tag_is_english() {
        assert_eq!(LangTag::default(), LangTag::english());
        assert_eq!(Provenance::default(), Provenance::Seed);
    }

    #[test]
    fn test_canonical_json_hash_is_stable_and_order_sensitive() {
        let a = canonical_json_hash(&("x", 1u32));
        let b = canonical_json_hash(&("x", 1u32));
        let c = canonical_json_hash(&("x", 2u32));
        assert_eq!(a, b, "同内容同哈希");
        assert_eq!(a.0.len(), 64, "sha256 全长 hex");
        assert_ne!(a, c, "内容变则哈希变");
    }

    #[test]
    fn test_provenance_as_str_matches_serde_snake_case() {
        for p in [
            Provenance::Seed,
            Provenance::Generated,
            Provenance::Imported,
            Provenance::Collected,
        ] {
            let serde_str = serde_json::to_value(p).expect("序列化").to_string();
            assert_eq!(
                serde_str,
                format!("\"{}\"", p.as_str()),
                "as_str 与 serde 同口径"
            );
        }
    }

    #[test]
    fn test_persist_enums_as_str_match_serde_snake_case() {
        for v in [
            Verdict::Correct,
            Verdict::Partial,
            Verdict::Wrong,
            Verdict::Acceptable,
            Verdict::Pending,
        ] {
            assert_eq!(
                serde_json::to_value(v).expect("序列化").to_string(),
                format!("\"{}\"", v.as_str())
            );
        }
        for g in [GradedBy::Rule, GradedBy::LlmReview, GradedBy::Human] {
            assert_eq!(
                serde_json::to_value(g).expect("序列化").to_string(),
                format!("\"{}\"", g.as_str())
            );
        }
        for m in [AttemptMode::Practice, AttemptMode::Typing] {
            assert_eq!(
                serde_json::to_value(m).expect("序列化").to_string(),
                format!("\"{}\"", m.as_str())
            );
        }
        assert_eq!(GradedBy::LlmReview.as_str(), "llm_review");
    }

    #[test]
    fn test_english_lang_tag_is_en() {
        assert_eq!(LangTag::english().0, "en");
        assert_eq!(LangTag::default(), LangTag::english());
    }

    #[test]
    fn test_enums_serialize_snake_case_in_json() {
        let q = Question::new(
            "choose".into(),
            QuestionKind::SingleChoice(test_choice_body()),
            1,
            vec![],
            Provenance::Seed,
        );
        let json = serde_json::to_string(&q.kind).expect("序列化题型");
        assert!(
            json.starts_with("{\"single_choice\""),
            "题型枚举 snake_case: {json}"
        );
        let ans = serde_json::to_string(&Answer::Choice(vec![0])).expect("序列化作答");
        assert!(ans.contains("choice"), "作答枚举 snake_case: {ans}");
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
            Provenance::Seed,
        );
        assert_eq!(q.stem, "choose");
        assert_eq!(q.difficulty, 1);
    }

    struct FakeBus;
    impl EventBus for FakeBus {
        fn publish(&self, ev: DomainEvent) {
            drop(ev);
        }
    }

    #[test]
    fn test_publish_feeds_subscriber() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Counting(AtomicUsize);
        impl EventBus for Counting {
            fn publish(&self, ev: DomainEvent) {
                drop(ev);
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let bus = Counting(AtomicUsize::new(0));
        bus.publish(DomainEvent::AttemptGraded {
            attempt_id: "a".into(),
            qid: Qid("q".into()),
            verdict: Verdict::Correct,
            correct: true,
        });
        assert_eq!(bus.0.load(Ordering::SeqCst), 1, "publish 送达订阅者");
        // EventBus 对象安全（Box<dyn EventBus> 可用）
        let dyn_bus: Box<dyn EventBus> = Box::new(FakeBus);
        dyn_bus.publish(DomainEvent::AttemptGraded {
            attempt_id: "b".into(),
            qid: Qid("q2".into()),
            verdict: Verdict::Wrong,
            correct: false,
        });
    }
}
