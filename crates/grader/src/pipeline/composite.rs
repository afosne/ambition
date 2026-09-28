//! composite 判分（M5 落地，docs/07 §3）：rubric 维度 Jev 打分 → 加权档位 → veto 降档
//! 单请求 fan-out：每维一条 `score` + 每 veto 一条 `noul`（**正面表述**：noul = P(性质成立)，< 0.5 触发）。
//! rubric 资产单源（corpus::rubric）：dims/权重/档位线/veto 全部来自版本化 JSON，代码只做合成与路由。

use crate::corpus::rubric::Rubric;
use crate::*;

/// Jev composite 判词（每维归一化分 + 触发的 veto + 综合置信 = 各维最小）
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeJudge {
    pub dims: Vec<DimScore>,
    pub triggered_vetoes: Vec<String>,
    pub confidence: f32,
}

impl Grader {
    /// 维度打分（M5：单请求 fan-out；无 Jev → Offline——调用方按离线矩阵降级，docs/07 §6）
    pub async fn jev_composite(
        &self,
        state: &JevState,
        rubric: &Rubric,
    ) -> Result<CompositeJudge, JevError> {
        let Some(jev) = &self.jev else {
            return Err(JevError::Offline);
        };
        let mut questions = JevQuestions::new();
        for dim in &rubric.dims {
            questions.insert(
                dim.key.clone(),
                JevQuestion::Score(ScoreQ {
                    instructions: dim_instructions(&dim.key),
                    levels: dim.levels_en.clone(),
                }),
            );
        }
        for key in &rubric.veto {
            questions.insert(
                format!("veto_{key}"),
                JevQuestion::Noul(NoulQ {
                    instructions: veto_instructions(key),
                }),
            );
        }
        let answers = jev.evaluate(state.clone(), &questions).await?;
        let mut dims = Vec::with_capacity(rubric.dims.len());
        let mut confidence = f32::MAX;
        for dim in &rubric.dims {
            let parsed = match answers.get(&dim.key) {
                Some(JevAnswer::Score { score, confidence }) => {
                    let scale = (dim.levels_en.len().saturating_sub(1)) as f32;
                    let norm = if scale > 0.0 {
                        (*score as f32 / scale).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    Some((norm, *confidence as f32))
                }
                _ => None,
            };
            // 缺答/型别不符 → 0 分 0 置信（保守：综合 confidence 被拖到 0 → Pending 桶）
            let (score, dim_confidence) = parsed.unwrap_or((0.0, 0.0));
            if parsed.is_none() {
                tracing::warn!(key = %dim.key, "composite dim missing or mistyped, scored zero");
            }
            confidence = confidence.min(dim_confidence);
            dims.push(DimScore {
                key: dim.key.clone(),
                score,
                confidence: dim_confidence,
            });
        }
        let mut triggered_vetoes = Vec::new();
        for key in &rubric.veto {
            let answer_id = format!("veto_{key}");
            match answers.get(&answer_id).and_then(JevAnswer::noul_prob) {
                Some(prob) if prob < 0.5 => triggered_vetoes.push(key.clone()),
                Some(_) => {}
                None => {
                    tracing::warn!(key = %answer_id, "veto answer missing, treated as not triggered")
                }
            }
        }
        Ok(CompositeJudge {
            dims,
            triggered_vetoes,
            confidence: if confidence == f32::MAX {
                1.0
            } else {
                confidence
            },
        })
    }
}

/// 维度打分指令（state 提供 prompt+essay 上下文；档位描述在 ScoreQ.levels——考纲原文，docs/16 §4）
fn dim_instructions(key: &str) -> String {
    format!(
        "Rate the essay's `{key}` dimension against the given levels, using the prompt and essay in state as evidence."
    )
}

/// veto 指令（**正面表述**：noul = P(性质成立)；noul < 0.5 触发降档，docs/07 §3）
fn veto_instructions(key: &str) -> String {
    match key {
        "off_topic" => "The essay stays on topic and addresses the prompt.".into(),
        "meaning_changed" => "The essay keeps a clear and undistorted meaning.".into(),
        "heavy_chinglish" => {
            "The essay does not read like heavy Chinglish (translationese).".into()
        }
        other => format!("The essay does not exhibit the issue: {other}."),
    }
}

/// 加权合成 → 档位分：按 rubric key 对齐权重（缺维按 0 分保守计入）→ ×max → 向下取档
/// （band_map 中 ≤ raw 的最大值；raw < 首档 → 0；docs/07 §3）
pub fn compose_scores(dims: &[DimScore], rubric: &Rubric) -> f32 {
    let weighted: f32 = rubric
        .dims
        .iter()
        .map(|want| {
            let got = dims
                .iter()
                .find(|have| have.key == want.key)
                .map(|d| d.score)
                .unwrap_or(0.0);
            got * want.weight
        })
        .sum();
    band_floor(weighted * rubric.max, &rubric.band_map)
}

/// raw 向下取档（band_map 中 ≤ raw 的最大值；空表/低于首档 → 0）
fn band_floor(raw: f32, band_map: &[f32]) -> f32 {
    band_map
        .iter()
        .copied()
        .filter(|band| *band <= raw)
        .fold(0.0f32, f32::max)
}

/// veto 一票降档：降到严格低于当前档的下一档（已是最低档 → 0；docs/07 §3）
pub fn down_band(band: f32, band_map: &[f32]) -> f32 {
    band_map
        .iter()
        .copied()
        .filter(|b| *b < band - f32::EPSILON)
        .fold(0.0f32, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::rubric::load_rubric;

    fn gaokao() -> &'static Rubric {
        load_rubric("gaokao25").expect("内置 gaokao25")
    }

    fn ielts() -> &'static Rubric {
        load_rubric("ielts_task2").expect("内置 ielts_task2")
    }

    /// 全维同分的维度集（keys 取自 rubric，长度对齐）
    fn uniform_dims(rubric: &Rubric, score: f32) -> Vec<DimScore> {
        rubric
            .dims
            .iter()
            .map(|d| DimScore {
                key: d.key.clone(),
                score,
                confidence: 0.9,
            })
            .collect()
    }

    #[test]
    fn test_compose_scores_floors_gaokao_to_band() {
        let r = gaokao();
        assert_eq!(
            compose_scores(&uniform_dims(r, 1.0), r),
            25.0,
            "满分 → 末档"
        );
        assert_eq!(
            compose_scores(&uniform_dims(r, 0.5), r),
            10.0,
            "12.5 → 下档 10"
        );
        assert_eq!(
            compose_scores(&uniform_dims(r, 0.1), r),
            0.0,
            "2.5 < 首档 5 → 0"
        );
        // 恰在档位线 15：0.6×25 = 15
        assert_eq!(compose_scores(&uniform_dims(r, 0.6), r), 15.0, "档位线含界");
        // 档位线下方 14.9：0.596×25 = 14.9 → 10
        assert_eq!(
            compose_scores(&uniform_dims(r, 0.596), r),
            10.0,
            "线下取下档"
        );
    }

    #[test]
    fn test_compose_scores_ielts_band_floor() {
        let r = ielts();
        assert_eq!(
            compose_scores(&uniform_dims(r, 1.0), r),
            9.0,
            "满分 → band 9"
        );
        assert_eq!(
            compose_scores(&uniform_dims(r, 0.5), r),
            4.0,
            "4.5 → floor 4"
        );
        assert_eq!(
            compose_scores(&uniform_dims(r, 0.4), r),
            0.0,
            "3.6 < 首档 4 → 0"
        );
    }

    #[test]
    fn test_compose_scores_missing_dim_counts_zero() {
        let r = gaokao();
        let mut dims = uniform_dims(r, 1.0);
        dims.pop(); // 丢最后一维（format, 权重 0.1）
        // 缺维按 0 计：raw = (1-0.1)×25 = 22.5 → 档 20
        assert_eq!(compose_scores(&dims, r), 20.0, "缺维保守计 0");
    }

    #[test]
    fn test_down_band_drops_one_band_and_stops_at_zero() {
        let map = &[5.0f32, 10.0, 15.0, 20.0, 25.0];
        assert_eq!(down_band(25.0, map), 20.0);
        assert_eq!(down_band(15.0, map), 10.0);
        assert_eq!(down_band(5.0, map), 0.0, "最低档再降 → 0");
        assert_eq!(down_band(0.0, map), 0.0, "已为 0 幂等");
        assert_eq!(down_band(17.0, map), 15.0, "非表内档 → 向下取档");
        let ielts_map = &[4.0f32, 5.0, 6.0, 7.0, 8.0, 9.0];
        assert_eq!(down_band(7.0, ielts_map), 6.0, "雅思 band 降档");
    }

    #[test]
    fn test_dim_instructions_and_veto_instructions_mention_state_and_keys() {
        let dim = dim_instructions("task");
        assert!(dim.contains("`task`"), "含维度 key");
        assert!(dim.contains("state"), "指向 state 上下文");
        let veto = veto_instructions("off_topic");
        assert!(veto.contains("on topic"), "正面表述（noul=P成立）");
        assert!(
            veto_instructions("custom_issue").contains("custom_issue"),
            "未知键透传"
        );
    }

    #[tokio::test]
    async fn test_jev_composite_scores_dims_and_flags_vetoes() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "task": {"type": "score", "score": 2.0, "confidence": 0.9},
                    "structure": {"type": "score", "score": 1.0, "confidence": 0.8},
                    "grammar": {"type": "score", "score": 2.0, "confidence": 0.95},
                    "lexicon": {"type": "score", "score": 1.0, "confidence": 0.7},
                    "coherence": {"type": "score", "score": 1.0, "confidence": 0.85},
                    "format": {"type": "score", "score": 2.0, "confidence": 0.99},
                    "veto_off_topic": {"type": "noul", "noul": 0.9},
                    "veto_meaning_changed": {"type": "noul", "noul": 0.4},
                    "veto_heavy_chinglish": {"type": "noul", "noul": 0.2}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(std::sync::Arc::new(
                crate::common::fixtures::client_with(&server).await,
            )),
            None,
            Thresholds::english(),
        );
        let rubric = gaokao();
        let state = JevState(serde_json::json!({
            "prompt": "Write about your favourite season.",
            "essay": "My favourite season is spring because..."
        }));
        let judge = grader
            .jev_composite(&state, rubric)
            .await
            .expect("composite 可判");
        assert_eq!(judge.dims.len(), rubric.dims.len(), "每维一答");
        let task = judge
            .dims
            .iter()
            .find(|d| d.key == "task")
            .expect("task 维");
        assert!((task.score - 1.0).abs() < 1e-5, "2.0/(3-1) 归一化 = 1.0");
        let structure = judge
            .dims
            .iter()
            .find(|d| d.key == "structure")
            .expect("structure 维");
        assert!((structure.score - 0.5).abs() < 1e-5, "1.0/2 = 0.5");
        assert_eq!(
            judge.triggered_vetoes,
            vec!["meaning_changed", "heavy_chinglish"],
            "noul<0.5 的两条触发、0.9 的 off_topic 不触发"
        );
        assert!(
            (judge.confidence - 0.7).abs() < 1e-5,
            "综合置信 = 各维最小（lexicon 0.7）"
        );
        // 合成链路：veto 降档
        let band = compose_scores(&judge.dims, rubric);
        let final_band = if judge.triggered_vetoes.is_empty() {
            band
        } else {
            down_band(band, &rubric.band_map)
        };
        assert!(final_band <= band, "veto 只降不升");
    }

    #[tokio::test]
    async fn test_jev_composite_offline_without_jev() {
        let grader = Grader::offline();
        let state = JevState(serde_json::json!({"prompt": "p", "essay": "e"}));
        let err = grader
            .jev_composite(&state, gaokao())
            .await
            .expect_err("无 Jev → Offline");
        assert!(matches!(err, ambition_jev::JevError::Offline));
    }

    #[test]
    fn test_band_floor_picks_greatest_band_not_above_raw() {
        let map = &[5.0f32, 10.0, 15.0];
        assert_eq!(band_floor(12.0, map), 10.0);
        assert_eq!(band_floor(15.0, map), 15.0, "含界");
        assert_eq!(band_floor(4.9, map), 0.0, "低于首档");
        assert_eq!(band_floor(99.0, &[]), 0.0, "空表防御");
    }

    #[test]
    fn test_veto_instructions_are_positive_statements() {
        assert!(
            veto_instructions("off_topic").contains("on topic"),
            "正面表述（noul=P 成立）"
        );
        assert!(
            veto_instructions("heavy_chinglish").contains("Chinglish"),
            "键→语义映射"
        );
        assert!(
            veto_instructions("custom_issue").contains("custom_issue"),
            "未知键透传"
        );
    }
}
