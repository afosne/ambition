//! 作答与判分载荷：Attempt/GradeResult/Arbitration/Timing/UserBaseline（docs/02 §3）

use super::*;

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

/// 判分结果（M1 规则层；M3 起由07 三级流水线产出）
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

/// LLM 第三级复核产出的**判断**（不打分，docs/07 §4；M3 落地）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Arbitration {
    /// 判断：学生答案可否接受
    pub acceptable: bool,
    /// 复核归出的错因（无错 = NoError）
    pub error_tag: ErrorTag,
    /// 给人看的一句理由（LLM 文本，不参与判分口径）
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arbitration_struct_is_serializable() {
        let arb = Arbitration {
            acceptable: true,
            error_tag: ErrorTag::Collocation,
            reason: "variant accepted".into(),
        };
        let json = serde_json::to_string(&arb).expect("序列化");
        assert!(json.contains("\"acceptable\":true"), "{json}");
        assert!(json.contains("collocation"), "{json}");
        let back: Arbitration = serde_json::from_str(&json).expect("反序列化");
        assert_eq!(back, arb);
    }
}
