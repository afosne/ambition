//! AI 成本台账载荷：UsageRecord 单条记账 / UsageTotal·UsageSummary 区间聚合（docs/02 §5、docs/05 §1）

/// 成本台账行（按 kind 聚合，docs/05 §1 usage_summary）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageTotal {
    pub kind: String,
    pub tokens_in: i64,
    pub tokens_out: i64,
}

/// 成本台账汇总（区间内各 kind + 总计；成本面板数据，M3 落地）
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct UsageSummary {
    pub rows: Vec<UsageTotal>,
    pub tokens_in: i64,
    pub tokens_out: i64,
}

impl UsageSummary {
    /// 由聚合行构造（总计 = 行求和；空区间 = 零值）
    pub fn from_rows(rows: Vec<UsageTotal>) -> Self {
        let tokens_in = rows.iter().map(|r| r.tokens_in).sum();
        let tokens_out = rows.iter().map(|r| r.tokens_out).sum();
        Self {
            rows,
            tokens_in,
            tokens_out,
        }
    }
}

/// 单条用量记账载荷（`AiLedger::record_usage` 与 store `record_usage` 共用——
/// 2026-09-27 跨模块收口：原 4 位置参数（kind/model/tokens_in/tokens_out）改单结构，
/// 跨模块调用只传 1 个对象，docs/05 §1）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageRecord {
    /// 记账维度（LlmKind::as_str 或 "jev"）
    pub kind: String,
    /// 模型标识
    pub model: String,
    /// 输入 tokens
    pub tokens_in: i64,
    /// 输出 tokens
    pub tokens_out: i64,
}

impl UsageRecord {
    /// 构造（kind = 记账维度，model = 模型标识）
    pub fn new(
        kind: impl Into<String>,
        model: impl Into<String>,
        tokens_in: i64,
        tokens_out: i64,
    ) -> Self {
        Self {
            kind: kind.into(),
            model: model.into(),
            tokens_in,
            tokens_out,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_usage_record_carries_fields_for_cross_module_call() {
        let record = UsageRecord::new("question_gen", "m1", 40, 12);
        assert_eq!(record.kind, "question_gen");
        assert_eq!(record.model, "m1");
        assert_eq!((record.tokens_in, record.tokens_out), (40, 12));
    }

    #[test]
    fn test_from_rows_sums_totals_and_keeps_default() {
        let empty = UsageSummary::from_rows(vec![]);
        assert_eq!((empty.tokens_in, empty.tokens_out), (0, 0));
        let summary = UsageSummary::from_rows(vec![
            UsageTotal {
                kind: "llm".into(),
                tokens_in: 100,
                tokens_out: 20,
            },
            UsageTotal {
                kind: "jev".into(),
                tokens_in: 50,
                tokens_out: 10,
            },
        ]);
        assert_eq!((summary.tokens_in, summary.tokens_out), (150, 30));
        assert_eq!(summary.rows.len(), 2);
        assert_eq!(UsageSummary::default().rows.len(), 0, "默认零值可用");
    }
}
