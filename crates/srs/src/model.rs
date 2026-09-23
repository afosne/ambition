//! 记忆状态与卡片模型：FsrsState/Card/Rating（FSRS 调度载荷，docs/08 §1）

use super::*;

/// 期望保持率（FSRS 调度目标，docs/08 §1）
pub const DESIRED_RETENTION: f32 = 0.9;

/// FSRS 记忆状态（stability/difficulty；持久化视图 —— fsrs::MemoryState 无 serde）
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FsrsState {
    pub stability: f32,
    pub difficulty: f32,
}

impl FsrsState {
    /// 转 fsrs-rs 调度入参
    pub fn to_memory(&self) -> MemoryState {
        MemoryState {
            stability: self.stability,
            difficulty: self.difficulty,
        }
    }

    /// 由 fsrs-rs 调度结果构造
    pub fn from_memory(memory: MemoryState) -> Self {
        Self {
            stability: memory.stability,
            difficulty: memory.difficulty,
        }
    }
}

/// 复习卡（srs_queue 载荷；docs/08 §1）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Card {
    pub card_id: CardId,
    pub source: CardSource,
    /// None = 新卡未复习
    pub fsrs: Option<FsrsState>,
    pub mode: ReviewMode,
    pub wrong_count: u32,
    pub guessed_count: u32,
    /// 错词本错因标签（合并去重）
    pub error_tags: Vec<ErrorTag>,
    /// 连续 Good/Easy 计数（升档原料）
    pub ok_streak: u32,
    /// unix 秒
    pub due_at: i64,
    pub last_review_at: Option<i64>,
    pub created_at: i64,
}

impl Card {
    /// 新卡：due_at = now（错词/假对立即进队列 = 提权，docs/08 §1）
    pub fn new(source: CardSource, now: i64) -> Self {
        Self {
            card_id: CardId::from_source(&source),
            source,
            fsrs: None,
            mode: ReviewMode::Choice,
            wrong_count: 0,
            guessed_count: 0,
            error_tags: Vec::new(),
            ok_streak: 0,
            due_at: now,
            last_review_at: None,
            created_at: now,
        }
    }
}

/// 复习评分（FSRS 四档）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}

impl Rating {
    /// 持久化字符串（snake_case，与 serde 同口径）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Again => "again",
            Self::Hard => "hard",
            Self::Good => "good",
            Self::Easy => "easy",
        }
    }

    /// TUI 键位 1-4 → 四档（docs/03 §5.3）
    pub fn from_key(digit: u8) -> Option<Self> {
        match digit {
            1 => Some(Self::Again),
            2 => Some(Self::Hard),
            3 => Some(Self::Good),
            4 => Some(Self::Easy),
            _ => None,
        }
    }
}

/// srs 错误（调度输入非法等；跨层包 EngineError::Srs）
#[derive(Debug, thiserror::Error)]
pub enum SrsError {
    #[error("fsrs schedule failed: {0}")]
    Fsrs(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_card_is_due_immediately_with_content_address() {
        let source = CardSource::WordSense {
            word_id: "bank".into(),
            sense_id: 1,
        };
        let card = Card::new(source, 1_000);
        assert_eq!(card.card_id, CardId("ws:bank:1".to_string()));
        assert_eq!(card.due_at, 1_000, "新卡立即到期（提权）");
        assert!(card.fsrs.is_none(), "新卡未复习");
        assert_eq!(card.mode, ReviewMode::Choice);
        assert_eq!((card.wrong_count, card.guessed_count), (0, 0));
    }

    #[test]
    fn test_from_key_and_rating_as_str_roundtrip() {
        for (digit, rating) in [
            (1, Rating::Again),
            (2, Rating::Hard),
            (3, Rating::Good),
            (4, Rating::Easy),
        ] {
            assert_eq!(Rating::from_key(digit), Some(rating));
            let s = rating.as_str();
            assert!(["again", "hard", "good", "easy"].contains(&s));
            let json = serde_json::to_string(&rating).expect("序列化");
            assert_eq!(json, format!("\"{s}\""), "serde 与 as_str 同口径");
        }
        assert_eq!(Rating::from_key(0), None);
        assert_eq!(Rating::from_key(9), None);
    }

    #[test]
    fn test_as_str_rating_matches_serde_snake_case() {
        // 配对测试：Rating::as_str 与 serde snake_case 同口径（与 from_key 往返各管各的钩子配对）
        assert_eq!(Rating::Again.as_str(), "again");
        assert_eq!(Rating::Hard.as_str(), "hard");
        assert_eq!(Rating::Good.as_str(), "good");
        assert_eq!(Rating::Easy.as_str(), "easy");
        let json = serde_json::to_string(&Rating::Good).expect("序列化");
        assert_eq!(json, "\"good\"");
    }

    #[test]
    fn test_to_memory_maps_stability_and_difficulty() {
        let state = FsrsState {
            stability: 3.5,
            difficulty: 4.25,
        };
        let memory = state.to_memory();
        assert_eq!(memory.stability, 3.5);
        assert_eq!(memory.difficulty, 4.25);
        let back = FsrsState::from_memory(memory);
        assert_eq!(back, state, "MemoryState 往返无损");
    }

    #[test]
    fn test_from_memory_restores_persisted_state() {
        let state = FsrsState {
            stability: 1.5,
            difficulty: 6.0,
        };
        let json = serde_json::to_string(&state).expect("序列化");
        let parsed: FsrsState = serde_json::from_str(&json).expect("反序列化");
        assert_eq!(parsed, state, "FsrsState 可持久化");
        assert_eq!(FsrsState::from_memory(parsed.to_memory()), state);
    }
}
