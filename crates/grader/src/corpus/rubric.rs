//! rubric 判分口径资产（M5，docs/07 §3、docs/16 §4）：`crates/grader/assets/rubrics/<id>.json` 内嵌
//! 判分语料红线 #19：不进 `data/`、`include_str!` 编译期内嵌、按 rubric_id/version 版本化——用户改源文件不生效。

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// rubric 单维定义（key + 权重 + 考纲原文档位描述——levels 即 Jev ScoreQ.levels）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RubricDim {
    /// 维度 key（与 composite DimScore.key 对齐）
    pub key: String,
    /// 权重（同 rubric 内求和 = 1.0）
    pub weight: f32,
    /// 档位描述原文（英文——Jev state 只进 l2，docs/05 §3.5）
    pub levels_en: Vec<String>,
}

/// 作文/翻译判分口径（docs/16 §4 schema；判分口径变更必升 version）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rubric {
    pub rubric_id: String,
    pub version: u32,
    /// 满分（band_map 末档 = max）
    pub max: f32,
    /// 档位分界线（升序；compose_scores 向下取档、down_band 降档的依据）
    pub band_map: Vec<f32>,
    pub dims: Vec<RubricDim>,
    /// veto 键（正面表述的 Noul 判断；noul < 0.5 触发降档）
    pub veto: Vec<String>,
}

impl Rubric {
    /// 按 key 取权重（缺 key = 0：维度漂移时 compose_scores 按 0 分保守计入）
    pub fn weight_of(&self, key: &str) -> f32 {
        self.dims
            .iter()
            .find(|d| d.key == key)
            .map(|d| d.weight)
            .unwrap_or(0.0)
    }
}

/// 内嵌 rubric 资产清单（M5 内置两轨：高考 25 分档 / 雅思 Task2 band）
const RUBRICS: &[(&str, &str)] = &[
    (
        "gaokao25",
        include_str!("../../assets/rubrics/gaokao25.json"),
    ),
    (
        "ielts_task2",
        include_str!("../../assets/rubrics/ielts_task2.json"),
    ),
];

/// 解析后的 rubric 表（每 id 一个单例；坏文件 tracing::warn 跳过不阻断——同语料口径 P-8 可见降级）
fn rubrics() -> &'static BTreeMap<String, Rubric> {
    static CACHE: OnceLock<BTreeMap<String, Rubric>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = BTreeMap::new();
        for (id, json) in RUBRICS {
            match serde_json::from_str::<Rubric>(json) {
                Ok(rubric) => {
                    map.insert(rubric.rubric_id.clone(), rubric);
                }
                Err(err) => tracing::warn!(error = %err, id, "rubric asset parse failed, skipped"),
            }
        }
        map
    })
}

/// 读 rubric（按 id；未知 id → None，调用方按题面 rubric_id / track 默认决定）
pub fn load_rubric(id: &str) -> Option<&'static Rubric> {
    rubrics().get(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rubrics_embedded_assets_parse_with_valid_shape() {
        let rubrics = rubrics();
        assert!(rubrics.contains_key("gaokao25"), "内置高考 rubric");
        assert!(rubrics.contains_key("ielts_task2"), "内置雅思 rubric");
        for rubric in rubrics.values() {
            let sum: f32 = rubric.dims.iter().map(|d| d.weight).sum();
            assert!(
                (sum - 1.0).abs() < 1e-6,
                "{} 权重和 =1（实际 {sum}）",
                rubric.rubric_id
            );
            assert!(rubric.version >= 1, "version 化");
            assert_eq!(
                *rubric.band_map.last().expect("非空 band_map"),
                rubric.max,
                "末档 = max"
            );
            assert!(
                rubric.band_map.windows(2).all(|w| w[0] < w[1]),
                "band_map 升序"
            );
            assert!(
                rubric.dims.iter().all(|d| d.levels_en.len() >= 2),
                "levels ≥2（score 有刻度）"
            );
        }
    }

    #[test]
    fn test_load_rubric_finds_builtin_and_misses_unknown() {
        let gaokao = load_rubric("gaokao25").expect("内置可读");
        assert_eq!(gaokao.rubric_id, "gaokao25");
        assert_eq!(gaokao.max, 25.0);
        assert_eq!(gaokao.band_map, vec![5.0, 10.0, 15.0, 20.0, 25.0]);
        assert!(load_rubric("no-such-rubric").is_none(), "未知 id → None");
    }

    #[test]
    fn test_weight_of_looks_up_by_key_and_defaults_zero() {
        let rubric = load_rubric("ielts_task2").expect("内置可读");
        assert!((rubric.weight_of("task_response") - 0.25).abs() < 1e-6);
        assert_eq!(rubric.weight_of("nope"), 0.0, "缺 key → 0（保守计入）");
    }
}
