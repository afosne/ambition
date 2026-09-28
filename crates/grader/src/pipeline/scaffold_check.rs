//! 脚手架自查（M5-W4，docs/10 §1）：Jev noul×2（meaning_kept/fits_prompt——**正面表述**、双 prob ≥0.5 过）
//! LLM 模板痕迹检测随 W5（批注波次复用同一 LLM 通路）；无 Jev → Offline（引擎按离线矩阵提示配置）。

use crate::*;
use ambition_writing::{ScaffoldCheck, Template};

impl Grader {
    /// 模板草稿自查（辅导性门槛 passed，非判分档位；永不 panic、缺答显式 Err）
    pub async fn check_scaffold(
        &self,
        essay: &str,
        t: &Template,
    ) -> Result<ScaffoldCheck, JevError> {
        let Some(jev) = &self.jev else {
            return Err(JevError::Offline);
        };
        let state = JevState(serde_json::json!({
            "essay": essay,
            "genre": t.genre,
            "track": t.track,
        }));
        let mut questions = JevQuestions::new();
        questions.insert(
            "meaning_kept".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "The draft keeps the original meaning of every filled slot and reads coherently.".into(),
            }),
        );
        questions.insert(
            "fits_prompt".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "The draft fits the genre and purpose of the template.".into(),
            }),
        );
        let answers = jev.evaluate(state, &questions).await?;
        let meaning_kept = answers
            .get("meaning_kept")
            .and_then(JevAnswer::noul_prob)
            .ok_or_else(|| JevError::Invalid("missing meaning_kept answer".into()))?
            as f32;
        let fits_prompt = answers
            .get("fits_prompt")
            .and_then(JevAnswer::noul_prob)
            .ok_or_else(|| JevError::Invalid("missing fits_prompt answer".into()))?
            as f32;
        Ok(ScaffoldCheck {
            meaning_kept,
            fits_prompt,
            passed: meaning_kept >= 0.5 && fits_prompt >= 0.5,
        })
    }

    /// 升级句核验（M5-W5，docs/10 §2：Jev noul「原意未变」≥0.5 过；<0.5 = 语义走样必须拦；无 Jev → Offline）
    pub async fn verify_upgrade(&self, from: &str, to: &str) -> Result<bool, JevError> {
        let Some(jev) = &self.jev else {
            return Err(JevError::Offline);
        };
        let state = JevState(serde_json::json!({
            "original": from,
            "upgraded": to,
        }));
        let mut questions = JevQuestions::new();
        questions.insert(
            "meaning_kept".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "The upgraded sentence keeps the original meaning unchanged; only the wording improved.".into(),
            }),
        );
        let answers = jev.evaluate(state, &questions).await?;
        let meaning_kept = answers
            .get("meaning_kept")
            .and_then(JevAnswer::noul_prob)
            .ok_or_else(|| JevError::Invalid("missing meaning_kept answer".into()))?;
        Ok(meaning_kept >= 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::client_with;
    use ambition_writing::{Slot, TemplateSource};

    fn demo_template() -> Template {
        Template {
            template_id: "t1".into(),
            track: "cet4".into(),
            genre: "argument".into(),
            skeleton: vec![ambition_writing::ParaFunc {
                key: "hook".into(),
                label_zh: "引出话题".into(),
                lines: vec!["{{thesis}}".into()],
            }],
            slots: vec![Slot {
                id: "thesis".into(),
                label_zh: "论点".into(),
                options_en: vec!["...".into()],
            }],
            phrases: vec![],
            anti_patterns: vec![],
            source: TemplateSource::Builtin,
        }
    }

    async fn check_with(noul_a: f64, noul_b: f64) -> Result<ScaffoldCheck, JevError> {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "meaning_kept": {"type": "noul", "noul": noul_a},
                    "fits_prompt": {"type": "noul", "noul": noul_b}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(std::sync::Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        grader
            .check_scaffold("draft essay body", &demo_template())
            .await
    }

    #[tokio::test]
    async fn test_check_scaffold_passes_when_both_probs_clear_line() {
        let check = check_with(0.9, 0.7).await.expect("可判");
        assert!(check.passed, "双项 ≥0.5 → 通过");
        assert!((check.meaning_kept - 0.9).abs() < 1e-5);
        assert!((check.fits_prompt - 0.7).abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_check_scaffold_fails_when_any_prob_below_line() {
        let check = check_with(0.9, 0.3).await.expect("可判");
        assert!(!check.passed, "任一项 <0.5 → 不通过");
        assert!(
            (check.fits_prompt - 0.3).abs() < 1e-5,
            "分项原样回传（提示用）"
        );
    }

    #[tokio::test]
    async fn test_verify_upgrade_passes_when_meaning_kept() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": { "meaning_kept": {"type": "noul", "noul": 0.9} }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(std::sync::Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let ok = grader
            .verify_upgrade("I think it is good.", "I hold that it is beneficial.")
            .await
            .expect("可核验");
        assert!(ok, "原意保留 ≥0.5 → 通过");
    }

    #[tokio::test]
    async fn test_verify_upgrade_blocks_meaning_shift() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": { "meaning_kept": {"type": "noul", "noul": 0.3} }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(std::sync::Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let ok = grader
            .verify_upgrade(
                "The plan saves money.",
                "The plan changes the goal entirely.",
            )
            .await
            .expect("可核验");
        assert!(!ok, "语义走样 <0.5 → 必须拦（checklist）");
    }

    #[tokio::test]
    async fn test_verify_upgrade_offline_without_jev() {
        let grader = Grader::offline();
        let err = grader
            .verify_upgrade("a", "b")
            .await
            .expect_err("无 Jev → Offline");
        assert!(matches!(err, JevError::Offline));
    }

    #[tokio::test]
    async fn test_check_scaffold_offline_without_jev() {
        let grader = Grader::offline();
        let err = grader
            .check_scaffold("x", &demo_template())
            .await
            .expect_err("无 Jev → Offline");
        assert!(matches!(err, JevError::Offline));
    }
}
