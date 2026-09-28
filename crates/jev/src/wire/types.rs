//! Jev wire 类型与三原语：JevState/JevQuestion/JevAnswer/ChoiceQ/ScoreQ/NoulQ（docs/05 §2）

use crate::*;

/// Jev 状态（只放 l2 文本；母语/中文永不进 state，docs/05 §3.5）
#[derive(Debug, Clone)]
pub struct JevState(pub serde_json::Value);

/// 三原语之一：选项判断
#[derive(Debug, Clone)]
pub struct ChoiceQ {
    pub instructions: String,
    pub criteria: BTreeMap<String, String>,
}

/// 三原语之一：按档位打分（criteria = 有序 levels，2–10 档）
#[derive(Debug, Clone)]
pub struct ScoreQ {
    pub instructions: String,
    pub levels: Vec<String>,
}

/// 三原语之一：是/否概率
#[derive(Debug, Clone)]
pub struct NoulQ {
    pub instructions: String,
}

/// 质检问题集（id → 原语；顺序确定便于测试断言）
pub type JevQuestions = BTreeMap<String, JevQuestion>;

/// 单个问题（一个判断）
#[derive(Debug, Clone)]
pub enum JevQuestion {
    Choice(ChoiceQ),
    Score(ScoreQ),
    Noul(NoulQ),
}

/// 质检回答集（与 questions 同 id）
pub type JevAnswers = BTreeMap<String, JevAnswer>;

/// 单个回答
#[derive(Debug, Clone, PartialEq)]
pub enum JevAnswer {
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Score {
        score: f64,
        confidence: f64,
    },
    Noul {
        /// 是的概率 ∈ [0,1]（wire 字段名 noul）
        yes_prob: f64,
    },
}

impl JevAnswer {
    /// 取 noul 概率（非 noul → None，调用方按缺答处理）
    pub fn noul_prob(&self) -> Option<f64> {
        match self {
            Self::Noul { yes_prob } => Some(*yes_prob),
            _ => None,
        }
    }

    /// 取分数（非 score → None）
    pub fn score_value(&self) -> Option<f64> {
        match self {
            Self::Score { score, .. } => Some(*score),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noul_prob_and_score_value_helpers() {
        let noul = JevAnswer::Noul { yes_prob: 0.7 };
        assert_eq!(noul.noul_prob(), Some(0.7));
        assert_eq!(noul.score_value(), None);
        let score = JevAnswer::Score {
            score: 1.5,
            confidence: 0.9,
        };
        assert_eq!(score.score_value(), Some(1.5));
        assert_eq!(score.noul_prob(), None);
    }

    #[test]
    fn test_score_value_reads_only_score_answers() {
        let choice = JevAnswer::Choice {
            choice: "a".into(),
            probabilities: BTreeMap::new(),
            confidence: 0.5,
        };
        assert_eq!(choice.score_value(), None);
        assert_eq!(
            JevAnswer::Score {
                score: 2.0,
                confidence: 1.0
            }
            .score_value(),
            Some(2.0)
        );
    }
}
