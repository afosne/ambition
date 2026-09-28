//! 打字数据模型（细分域，docs/04-typing §数据）：击键与统计载荷

use serde::{Deserialize, Serialize};

/// 一次击键
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStroke {
    pub key: char,
    pub t_ms: u64,
    pub expected: char,
    pub correct: bool,
}

/// 打字统计
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TypingStats {
    pub wpm: f32,
    pub accuracy: f32,
    pub correct: usize,
    pub total: usize,
}
