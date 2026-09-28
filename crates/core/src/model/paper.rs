//! 套卷类型（M5-W6，docs/06 §6 契约）：PaperSpec/PaperSection/Paper——类型在 core（store/gen/engine 共用），组卷在 engine::paper
/// 卷内分区（题型子集 + 题量 + 单题分值；kinds 为 kind 键，docs/06 §6）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaperSection {
    pub title: String,
    pub kinds: Vec<String>,
    pub count: u16,
    pub points_each: f32,
}

/// 套卷规格（组卷输入；真题导入/自定义 spec 随16-config，MVP 默认卷见 cet4_default）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaperSpec {
    pub track: String,
    pub title: String,
    pub sections: Vec<PaperSection>,
    pub minutes: u16,
}

/// 组好的卷（paper_id = spec 内容哈希——同 spec 同卷幂等；qids 按 section 顺序稳定）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Paper {
    pub paper_id: String,
    pub spec: PaperSpec,
    pub qids: Vec<String>,
}

impl PaperSpec {
    /// 卷面总分（sections 题量 × 单题分——交卷换算基准）
    pub fn total_points(&self) -> f32 {
        self.sections
            .iter()
            .map(|s| f32::from(s.count) * s.points_each)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_total_points_sums_sections() {
        let spec = PaperSpec {
            track: "cet4".to_string(),
            title: "t".to_string(),
            sections: vec![
                PaperSection {
                    title: "I".to_string(),
                    kinds: vec!["single_choice".to_string()],
                    count: 10,
                    points_each: 2.0,
                },
                PaperSection {
                    title: "II".to_string(),
                    kinds: vec!["fill_blank".to_string()],
                    count: 5,
                    points_each: 2.0,
                },
            ],
            minutes: 30,
        };
        assert_eq!(spec.total_points(), 30.0, "10×2 + 5×2");
        let empty = PaperSpec {
            track: "x".to_string(),
            title: "t".to_string(),
            sections: vec![],
            minutes: 0,
        };
        assert_eq!(empty.total_points(), 0.0, "空卷 0 分");
    }
}
