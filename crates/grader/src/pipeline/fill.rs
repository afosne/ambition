//! Jev 填空/多选升级：grade_fill_jev/jev_blank/jev_multi_items（翻案 Acceptable/Partial、多选逐项 noul）

use super::*;

impl Grader {
    /// 填空 Jev 升级：逐空 acceptable 判（错空才问）→ 翻案/确认/灰区；确认错的空带错因
    pub(crate) async fn grade_fill_jev(
        &self,
        question: &Question,
        answer: &Answer,
        mut result: GradeResult,
    ) -> GradeResult {
        let thresholds = self.effective_thresholds(question);
        let mut min_margin = f32::MAX;
        let rule_score = result.score;
        let max = result.max;
        let mut flipped = 0usize;
        let mut undecided = false;
        for slot in 0..result.items.len() {
            if result.items[slot].correct {
                continue;
            }
            let blank_index = result.items[slot].index;
            let typed = match answer {
                Answer::Fill(values) => values.get(blank_index).cloned().unwrap_or_default(),
                _ => String::new(),
            };
            let judge = match self.jev_blank(question, blank_index, &typed).await {
                Ok(judge) => judge,
                Err(err) => {
                    // Jev 中途失败 = Jev 离线：保持规则结果（离线矩阵），标签缺失可见
                    tracing::warn!(error = %err, "jev_blank failed, keeping rule result");
                    return result;
                }
            };
            min_margin = min_margin.min(judge.yes_prob);
            match self.downgrade_route(route_from_margin(
                judge.yes_prob,
                thresholds.accept,
                thresholds.reject,
            )) {
                Route::AutoAccept => {
                    result.items[slot].correct = true;
                    flipped += 1;
                }
                Route::AutoReject => {
                    for tag in judge.error_tags {
                        if !result.error_tags.contains(&tag) {
                            result.error_tags.push(tag);
                        }
                    }
                }
                Route::LlmReview => undecided = true,
                Route::HumanReview => {
                    return pending_grade(rule_score, max, result.items, result.error_tags);
                }
            }
        }
        if undecided {
            return self
                .third_level(
                    question,
                    answer,
                    Provisional {
                        score: rule_score,
                        max,
                        items: result.items,
                        error_tags: result.error_tags,
                    },
                )
                .await;
        }
        let score = rule_score + flipped as f32;
        let verdict = if flipped == 0 {
            Verdict::Wrong // 全部确认错：维持规则结论
        } else if score >= max {
            Verdict::Acceptable // Jev 模糊通过
        } else {
            Verdict::Partial
        };
        GradeResult {
            verdict,
            score,
            max,
            confidence: if min_margin == f32::MAX {
                result.confidence
            } else {
                min_margin
            },
            graded_by: GradedBy::Jev,
            items: result.items,
            error_tags: result.error_tags,
        }
    }

    /// 单空可接受性 + 错因（一次请求；docs/07 §3 jev_blank；无 Jev → Offline）
    pub async fn jev_blank(
        &self,
        question: &Question,
        blank_index: usize,
        typed: &str,
    ) -> Result<BlankJudge, JevError> {
        let QuestionKind::FillBlank(fill) = &question.kind else {
            return Err(JevError::Invalid(
                "jev_blank requires a fill_blank question".into(),
            ));
        };
        let blank = fill
            .blanks
            .get(blank_index)
            .ok_or_else(|| JevError::Invalid(format!("blank {blank_index} out of range")))?;
        let Some(jev) = &self.jev else {
            return Err(JevError::Offline);
        };
        let state = JevState(serde_json::json!({
            "sentence": question.stem,
            "reference_answers": blank.acceptable,
            "student_answer": typed,
            "difficulty": question.difficulty,
        }));
        let mut questions = JevQuestions::new();
        questions.insert(
            "acceptable".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "The student's answer fits the sentence grammatically and preserves the intended meaning of the blank.".into(),
            }),
        );
        let questions = merge_error_cause(&question.lang.0, questions);
        let answers = jev.evaluate(state, &questions).await?;
        let yes_prob = answers
            .get("acceptable")
            .and_then(|a| a.noul_prob())
            .ok_or_else(|| JevError::Invalid("missing acceptable answer".into()))?
            as f32;
        Ok(BlankJudge {
            blank_index,
            yes_prob,
            error_tags: tag_error_cause(&answers, self.thresholds.accept),
        })
    }

    /// 多选漏选项逐项 noul + 错因（一次请求；docs/07 §3 jev_multi_items；无 Jev → Offline）
    pub async fn jev_multi_items(
        &self,
        question: &Question,
        selected: &[u32],
    ) -> Result<MultiJudge, JevError> {
        let QuestionKind::MultiChoice(body) = &question.kind else {
            return Err(JevError::Invalid(
                "jev_multi_items requires a multi_choice question".into(),
            ));
        };
        let missed: Vec<u32> = body
            .answer
            .iter()
            .filter(|id| !selected.contains(id))
            .copied()
            .collect();
        if missed.is_empty() {
            return Err(JevError::Invalid("no missed options to judge".into()));
        }
        let Some(jev) = &self.jev else {
            return Err(JevError::Offline);
        };
        let state = answer_state(question, &Answer::Choice(selected.to_vec()));
        let mut extra: Vec<(String, JevQuestion)> = Vec::with_capacity(missed.len());
        for id in &missed {
            extra.push((
                format!("belongs_{id}"),
                JevQuestion::Noul(NoulQ {
                    instructions:
                        "This option (see options in state) is part of the fully correct answer set; a thorough student should select it.".into(),
                }),
            ));
        }
        let questions = error_cause_questions(&question.lang.0, extra);
        let answers = jev.evaluate(state, &questions).await?;
        let mut items = Vec::with_capacity(missed.len());
        for id in missed {
            let yes_prob = answers
                .get(&format!("belongs_{id}"))
                .and_then(|a| a.noul_prob())
                .ok_or_else(|| JevError::Invalid(format!("missing belongs_{id}")))?;
            items.push(ItemJudge {
                index: id,
                belongs: yes_prob >= 0.5,
                margin: yes_prob as f32,
            });
        }
        Ok(MultiJudge {
            items,
            error_tags: tag_error_cause(&answers, self.thresholds.accept),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::*;

    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client_with(server: &MockServer) -> JevClient {
        JevClient::with_options(
            &format!("{}/v1", server.uri()),
            ambition_jev::DEFAULT_JEV_MODEL,
            Some("k".into()),
            None,
        )
    }

    #[tokio::test]
    async fn test_grade_fill_jev_flips_variant_to_acceptable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "acceptable": {"type": "noul", "noul": 0.9},
                    "error_cause": {"type": "choice", "choice": "no_error",
                                    "probabilities": {"no_error": 0.9}, "confidence": 0.8}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let grade = grader
            .grade(&fill_question(), &Answer::Fill(vec!["be".to_string()]))
            .await;
        assert_eq!(grade.verdict, Verdict::Acceptable, "变体经 Jev 翻案");
        assert_eq!((grade.score, grade.max), (1.0, 1.0));
        assert_eq!(grade.graded_by, GradedBy::Jev);
        assert!(grade.error_tags.is_empty(), "翻案成功不带错因");
    }

    #[tokio::test]
    async fn test_grade_fill_jev_confirms_wrong_with_tag_and_partial() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "acceptable": {"type": "noul", "noul": 0.1},
                    "error_cause": {"type": "choice", "choice": "article",
                                    "probabilities": {"article": 0.9}, "confidence": 0.88}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let grade = grader
            .grade(&fill_question(), &Answer::Fill(vec!["was".to_string()]))
            .await;
        assert_eq!(grade.verdict, Verdict::Wrong, "确认错：维持规则结论");
        assert_eq!(grade.graded_by, GradedBy::Jev);
        assert_eq!(
            grade.error_tags,
            vec![ErrorTag::Article],
            "确认错的空带错因"
        );
        assert!((grade.confidence - 0.1).abs() < 1e-5, "置信 = 最弱 margin");
    }

    #[tokio::test]
    async fn test_grade_fill_jev_gray_band_goes_pending_without_llm() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {"acceptable": {"type": "noul", "noul": 0.5}}
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let grade = grader
            .grade(&fill_question(), &Answer::Fill(vec!["was".to_string()]))
            .await;
        assert_eq!(grade.verdict, Verdict::Pending, "灰区无 LLM → 桶");
    }

    #[tokio::test]
    async fn test_jev_blank_requires_fill_question_and_range() {
        let grader = offline_router();
        let err = grader
            .jev_blank(&choice_question(), 0, "x")
            .await
            .expect_err("非填空题拒绝");
        assert!(matches!(err, JevError::Invalid(_)), "{err}");
        let err2 = grader
            .jev_blank(&fill_question(), 5, "x")
            .await
            .expect_err("越界拒绝");
        assert!(matches!(err2, JevError::Invalid(_)), "{err2}");
        let err3 = grader
            .jev_blank(&fill_question(), 0, "x")
            .await
            .expect_err("校验通过但离线 → Offline 显式");
        assert!(matches!(err3, JevError::Offline), "{err3}");
    }

    #[tokio::test]
    async fn test_jev_blank_parses_noul_and_tags() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "acceptable": {"type": "noul", "noul": 0.82},
                    "error_cause": {"type": "choice", "choice": "spelling",
                                    "probabilities": {"spelling": 0.8}, "confidence": 0.9}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let judge = grader
            .jev_blank(&fill_question(), 0, "is")
            .await
            .expect("逐空判断");
        assert_eq!(judge.blank_index, 0);
        assert!((judge.yes_prob - 0.82).abs() < 1e-5);
        assert_eq!(judge.error_tags, vec![ErrorTag::Spelling]);
    }

    #[tokio::test]
    async fn test_jev_blank_missing_answer_reports_invalid() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {"unexpected": {"type": "noul", "noul": 0.5}}
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let err = grader
            .jev_blank(&fill_question(), 0, "is")
            .await
            .expect_err("缺 acceptable 显式报错");
        assert!(matches!(err, JevError::Invalid(_)), "{err}");
    }

    #[tokio::test]
    async fn test_jev_multi_items_requires_multi_and_missed_options() {
        let grader = offline_router();
        let err = grader
            .jev_multi_items(&choice_question(), &[0])
            .await
            .expect_err("单选拒绝");
        assert!(matches!(err, JevError::Invalid(_)), "{err}");
        let err2 = grader
            .jev_multi_items(&multi_question(), &[0, 2])
            .await
            .expect_err("无漏选拒绝");
        assert!(matches!(err2, JevError::Invalid(_)), "{err2}");
        let err3 = grader
            .jev_multi_items(&multi_question(), &[0])
            .await
            .expect_err("校验通过但离线 → Offline 显式");
        assert!(matches!(err3, JevError::Offline), "{err3}");
    }

    #[tokio::test]
    async fn test_jev_multi_items_builds_per_missed_judges() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "belongs_2": {"type": "noul", "noul": 0.88},
                    "error_cause": {"type": "choice", "choice": "logic",
                                    "probabilities": {"logic": 0.8}, "confidence": 0.8}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let judge = grader
            .jev_multi_items(&multi_question(), &[0])
            .await
            .expect("逐项判断");
        assert_eq!(judge.items.len(), 1, "只判漏选项");
        assert_eq!(judge.items[0].index, 2);
        assert!(judge.items[0].belongs, "p≥0.5 → 属于正确集");
        assert!((judge.items[0].margin - 0.88).abs() < 1e-5);
        assert_eq!(judge.error_tags, vec![ErrorTag::Logic]);
    }
}
