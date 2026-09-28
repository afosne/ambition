//! 补练推荐 + 亮点短语库（M5-W5，docs/10 §2：缺陷→模板/句式/搭配补练；phrase_bank 语域分档）

use ambition_core::{ErrorTag, GradeDetail, GradeResult};

use crate::template::{Phrase, builtin_templates};

/// 推荐类型（模板重写 / 句式升级 / 搭配补练）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendKind {
    Template,
    Phrase,
    Collocation,
}

/// 一条补练推荐（TUI 结构化讲解区直显）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Recommendation {
    pub kind: RecommendKind,
    pub text_zh: String,
    pub reason_zh: String,
}

/// 错因 → 推荐类型（无映射 = 不硬推——拼写/审题/时间类不走句式补练）
fn kind_for_tag(tag: &ErrorTag) -> Option<RecommendKind> {
    use ErrorTag::*;
    match tag {
        Chinglish | PatternMisuse | Register => Some(RecommendKind::Template),
        Collocation | Preposition | WordSense | Confusable | Countability => {
            Some(RecommendKind::Collocation)
        }
        Cohesion | Reference | ClauseLinker | Tense | Sva | Article | NonFinite
        | SentenceStructure | Voice | PosMix | Logic | Redundancy => Some(RecommendKind::Phrase),
        _ => None,
    }
}

/// 最弱维度 → 推荐类型（composite Dims：分低者给对应补练）
fn kind_for_dim(key: &str) -> Option<RecommendKind> {
    match key {
        "task" | "task_response" => Some(RecommendKind::Template),
        "lexicon" | "lexical_resource" => Some(RecommendKind::Collocation),
        "coherence" | "coherence_cohesion" | "grammar" | "grammatical_range" | "structure" => {
            Some(RecommendKind::Phrase)
        }
        _ => None,
    }
}

/// 缺陷→补练推荐（纯函数；错因驱动优先、composite 最弱维度兜底；去重按类型、至多 3 条）
/// `lib` = `phrase_bank(track)` 结果（空库 → 文案退化为通用建议，不 panic）
pub fn recommend_after(g: &GradeResult, lib: &[Phrase]) -> Vec<Recommendation> {
    let mut reasons: Vec<(RecommendKind, String)> = Vec::new();
    // 1) 错因驱动（规则/Jev 侧 error_tags）
    for tag in &g.error_tags {
        if let Some(kind) = kind_for_tag(tag) {
            let reason = format!("错因：{}", tag.label_zh());
            if !reasons.iter().any(|(k, _)| *k == kind) {
                reasons.push((kind, reason));
            }
        }
    }
    // 2) 最弱维度驱动（composite Dims）
    if let GradeDetail::Dims(dims) = &g.detail
        && let Some(weak) = dims
            .iter()
            .filter(|d| d.score < 0.75)
            .min_by(|a, b| a.score.total_cmp(&b.score))
        && let Some(kind) = kind_for_dim(&weak.key)
    {
        let reason = format!("最弱维度：{}（{:.0}%）", weak.key, weak.score * 100.0);
        if !reasons.iter().any(|(k, _)| *k == kind) {
            reasons.push((kind, reason));
        }
    }
    reasons
        .into_iter()
        .take(3)
        .map(|(kind, reason_zh)| {
            let text_zh = match kind {
                RecommendKind::Template => {
                    "先在模板屏 Full 阶段用高级句替换该表达，再切半模板复写一遍（反模板重写）"
                        .to_string()
                }
                RecommendKind::Phrase | RecommendKind::Collocation => lib
                    .first()
                    .map(|p| format!("仿写亮点句式：{}", p.text))
                    .unwrap_or_else(|| "用 phrase_bank 中的同档短语仿写两句".to_string()),
            };
            Recommendation {
                kind,
                text_zh,
                reason_zh,
            }
        })
        .collect()
}

/// 亮点短语库（内置模板 phrases 汇聚，语域按 level 升序、按文本去重；08 补个人库）
pub fn phrase_bank(track: &str) -> Vec<Phrase> {
    let mut out: Vec<Phrase> = builtin_templates()
        .iter()
        .filter(|t| t.track == track)
        .flat_map(|t| t.phrases.iter().cloned())
        .collect();
    out.sort_by_key(|p| p.level);
    out.dedup_by(|a, b| a.text == b.text);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{DimScore, GradeDetail, GradedBy, Verdict};

    fn grade(error_tags: Vec<ErrorTag>, dims: Vec<DimScore>) -> GradeResult {
        GradeResult {
            verdict: Verdict::Partial,
            score: 15.0,
            max: 25.0,
            confidence: 0.8,
            graded_by: GradedBy::Jev,
            detail: GradeDetail::Dims(dims),
            error_tags,
        }
    }

    fn dim(key: &str, score: f32) -> DimScore {
        DimScore {
            key: key.to_string(),
            score,
            confidence: 0.8,
        }
    }

    #[test]
    fn test_recommend_after_maps_error_tags_and_dedups_by_kind() {
        let g = grade(
            vec![
                ErrorTag::Chinglish,
                ErrorTag::Collocation,
                ErrorTag::Spelling,
            ],
            vec![dim("lexicon", 0.5)],
        );
        let recs = recommend_after(&g, &[]);
        // Chinglish→Template、Collocation→Collocation、Spelling→None；lexicon 0.5 与 Collocation 去重
        assert_eq!(recs.len(), 2, "去重后至多两路: {recs:?}");
        assert_eq!(recs[0].kind, RecommendKind::Template);
        assert_eq!(recs[1].kind, RecommendKind::Collocation);
        assert!(recs.iter().all(|r| !r.reason_zh.is_empty()), "理由非空");
    }

    #[test]
    fn test_recommend_after_uses_dims_when_no_tags_and_respects_threshold() {
        let g = grade(vec![], vec![dim("grammar", 0.4), dim("lexicon", 0.9)]);
        let recs = recommend_after(&g, &[]);
        assert_eq!(recs.len(), 1, "只推最弱且 <0.75 的维");
        assert_eq!(recs[0].kind, RecommendKind::Phrase, "grammar 弱 → 句式");
        assert!(
            recs[0].reason_zh.contains("grammar"),
            "维度理由: {:?}",
            recs[0]
        );

        let strong = grade(vec![], vec![dim("grammar", 0.9)]);
        assert!(recommend_after(&strong, &[]).is_empty(), "无缺陷不硬推");
    }

    #[test]
    fn test_kind_for_tag_maps_grouping_and_skips_exam_noise() {
        use ErrorTag::*;
        assert_eq!(kind_for_tag(&Chinglish), Some(RecommendKind::Template));
        assert_eq!(kind_for_tag(&Collocation), Some(RecommendKind::Collocation));
        assert_eq!(kind_for_tag(&Tense), Some(RecommendKind::Phrase));
        assert_eq!(kind_for_tag(&Spelling), None, "拼写类不硬推句式补练");
        assert_eq!(kind_for_tag(&TimePressure), None, "应试类不硬推");
    }

    #[test]
    fn test_kind_for_dim_covers_composite_keys() {
        assert_eq!(kind_for_dim("task"), Some(RecommendKind::Template));
        assert_eq!(kind_for_dim("lexicon"), Some(RecommendKind::Collocation));
        assert_eq!(kind_for_dim("coherence"), Some(RecommendKind::Phrase));
        assert_eq!(kind_for_dim("mystery"), None, "未知维不映射");
    }

    #[test]
    fn test_phrase_bank_collects_tracks_sorted_and_deduped() {
        let bank = phrase_bank("cet4");
        assert!(!bank.is_empty(), "内置 cet4 模板短语在场");
        assert!(bank.iter().all(|p| p.level >= 1));
        let levels: Vec<u8> = bank.iter().map(|p| p.level).collect();
        let mut sorted = levels.clone();
        sorted.sort();
        assert_eq!(levels, sorted, "语域按 level 升序");
        assert!(phrase_bank("no-such-track").is_empty(), "未知轨空");
    }
}
