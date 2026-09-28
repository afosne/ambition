//! 作答与判分载荷：Attempt/GradeResult/Arbitration/Timing/UserBaseline（docs/02 §3）

use crate::*;

/// 作答（M1：选择 / 填空；M4 题型补全：Group 复合子题答案——Reading 递归判分）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Choice(Vec<u32>),
    Fill(Vec<String>),
    /// 复合题子题答案（逐子题递归判分，docs/07 §1 M4 注记）
    Group(Vec<Answer>),
    /// 文本作答（翻译/作文，M5-W2/W3——单段文本，走 composite 判分）
    Text(String),
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

/// 维度分（M5 composite：每维归一化 0..1 + 置信度；docs/02 §1、docs/07 §3）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DimScore {
    /// 维度 key（与 rubric dims 对齐）
    pub key: String,
    /// 归一化分 0..1（Jev score/(levels-1)）
    pub score: f32,
    /// 该维置信度（综合 confidence = 各维最小）
    pub confidence: f32,
}

/// 判分明细（docs/02 §1：逐项 / 维度 / 无明细——M5 起 detail 取代旧 items 字段）
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum GradeDetail {
    /// 逐项判定（多选/填空/复合题子题）
    Items(Vec<ItemResult>),
    /// 维度分（作文/翻译 composite，M5）
    Dims(Vec<DimScore>),
    /// 无明细（规则直判/待裁定）
    #[default]
    Plain,
}

/// 判分结果（M1 规则层；M3 起由 07 三级流水线产出）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GradeResult {
    pub verdict: Verdict,
    pub score: f32,
    pub max: f32,
    pub confidence: f32,
    pub graded_by: GradedBy,
    /// 判分明细（M5 起取代 items：旧 grades.payload 无此字段 → Plain 兼容读取）
    #[serde(default)]
    pub detail: GradeDetail,
    pub error_tags: Vec<ErrorTag>,
}

impl GradeResult {
    /// 逐项判定视图（Items 变体；其它变体回空表——渲染/汇总共用入口）
    pub fn items(&self) -> &[ItemResult] {
        match &self.detail {
            GradeDetail::Items(v) => v,
            _ => &[],
        }
    }

    /// 取出逐项判定并暂置 Plain（规则→Jev 升级的就地改写；改完用 set_items 还原，
    /// 非 Items 变体原样放回不丢明细）
    pub fn take_items(&mut self) -> Vec<ItemResult> {
        match std::mem::replace(&mut self.detail, GradeDetail::Plain) {
            GradeDetail::Items(v) => v,
            other => {
                self.detail = other;
                Vec::new()
            }
        }
    }

    /// 写回逐项判定（与 take_items 配对）
    pub fn set_items(&mut self, items: Vec<ItemResult>) {
        self.detail = GradeDetail::Items(items);
    }
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

    fn plain_grade() -> GradeResult {
        GradeResult {
            verdict: Verdict::Correct,
            score: 1.0,
            max: 1.0,
            confidence: 1.0,
            graded_by: GradedBy::Rule,
            detail: GradeDetail::Plain,
            error_tags: vec![],
        }
    }

    #[test]
    fn test_items_slice_shows_only_items_variant() {
        let mut g = plain_grade();
        assert!(g.items().is_empty(), "Plain = 空表");
        g.set_items(vec![ItemResult {
            index: 0,
            correct: true,
        }]);
        assert_eq!(g.items().len(), 1);
        assert!(g.items()[0].correct);
        let dims = GradeResult {
            detail: GradeDetail::Dims(vec![DimScore {
                key: "task".into(),
                score: 0.8,
                confidence: 0.9,
            }]),
            ..plain_grade()
        };
        assert!(dims.items().is_empty(), "Dims 不冒充 Items");
    }

    #[test]
    fn test_take_items_restores_other_detail_untouched() {
        let mut g = plain_grade();
        g.set_items(vec![ItemResult {
            index: 1,
            correct: false,
        }]);
        let taken = g.take_items();
        assert_eq!(taken.len(), 1);
        assert!(matches!(g.detail, GradeDetail::Plain), "取走后暂置 Plain");
        g.detail = GradeDetail::Dims(vec![]);
        let none = g.take_items();
        assert!(none.is_empty());
        assert!(
            matches!(g.detail, GradeDetail::Dims(_)),
            "非 Items 变体原样放回不丢明细"
        );
    }

    #[test]
    fn test_set_items_writes_items_variant_back() {
        let mut g = plain_grade();
        g.set_items(vec![ItemResult {
            index: 3,
            correct: false,
        }]);
        assert!(
            matches!(g.detail, GradeDetail::Items(_)),
            "set_items 写回 Items 变体"
        );
        assert_eq!(g.items()[0].index, 3, "写回后可经 items() 读出");
        g.set_items(vec![]);
        assert!(
            matches!(g.detail, GradeDetail::Items(_)),
            "空表仍是 Items（与 Plain 区分语义）"
        );
        assert!(g.items().is_empty());
    }

    #[test]
    fn test_grade_result_detail_defaults_for_legacy_json() {
        // 旧 grades.payload（无 detail 字段）兼容读取：M1-M4 历史行 → Plain
        let legacy = r#"{"verdict":"correct","score":1.0,"max":1.0,"confidence":1.0,"graded_by":"rule","error_tags":[]}"#;
        let grade: GradeResult = serde_json::from_str(legacy).expect("旧载荷可读");
        assert!(
            matches!(grade.detail, GradeDetail::Plain),
            "缺 detail → 默认 Plain"
        );
        let json = serde_json::to_string(&plain_grade()).expect("序列化");
        assert!(json.contains("\"detail\":"), "新载荷带 detail: {json}");
    }

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
