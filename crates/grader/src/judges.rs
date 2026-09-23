//! Jev 判词载荷：BlankJudge/ItemJudge/MultiJudge/Provisional（docs/07 §3）

use super::*;

/// 逐空判断（jev_blank 产出：可接受概率 + 该空错因）
#[derive(Debug, Clone, PartialEq)]
pub struct BlankJudge {
    pub blank_index: usize,
    pub yes_prob: f32,
    pub error_tags: Vec<ErrorTag>,
}

/// 漏选项判断（jev_multi_items：belongs = 模型倾向该选项属于正确答案集）
#[derive(Debug, Clone, PartialEq)]
pub struct ItemJudge {
    pub index: u32,
    pub belongs: bool,
    pub margin: f32,
}

/// 多选漏选判断集（逐项 + 错因，docs/07 §3）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MultiJudge {
    pub items: Vec<ItemJudge>,
    pub error_tags: Vec<ErrorTag>,
}

/// 第三级输入：暂定判定态（可回退的分与中间产物；灰区/离线时交 LLM 或进桶）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Provisional {
    pub score: f32,
    pub max: f32,
    pub items: Vec<ItemResult>,
    pub error_tags: Vec<ErrorTag>,
}

impl Provisional {
    /// 基本分构造（items/tags 默认空）
    pub fn new(score: f32, max: f32) -> Self {
        Self {
            score,
            max,
            items: Vec::new(),
            error_tags: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_provisional_starts_with_empty_items_and_tags() {
        let p = Provisional::new(1.5, 2.0);
        assert_eq!(p.score, 1.5);
        assert_eq!(p.max, 2.0);
        assert!(p.items.is_empty());
        assert!(p.error_tags.is_empty());
    }
}
