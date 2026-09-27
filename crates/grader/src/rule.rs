//! 一二级判定环：grade 分发 → grade_rule_gap → grade_after_rule/apply_error_tags（规则优先、零成本）

use super::*;

impl Grader {
    /// 三级流水线入口（①规则 → ②Jev → ③LLM/桶；永不 Err）
    pub async fn grade(&self, question: &Question, answer: &Answer) -> GradeResult {
        let (ids, fills) = split_answer(answer);
        match grade_rule(&question.kind, &ids, &fills) {
            None => self.grade_rule_gap(question, answer, &ids).await,
            Some(result) => self.grade_after_rule(question, answer, result).await,
        }
    }

    /// 规则不可判（多选漏选）：Jev 逐项 → LLM 复核 → 待裁定桶
    async fn grade_rule_gap(
        &self,
        question: &Question,
        answer: &Answer,
        selected: &[u32],
    ) -> GradeResult {
        let thresholds = self.effective_thresholds(question);
        let max = match &question.kind {
            QuestionKind::MultiChoice(body) => body.answer.len() as f32,
            _ => 1.0,
        };
        // 漏选 = 严格子集：已选项全部命中，暂定分 = 已选数
        let provisional = selected.len() as f32;
        if self.jev.is_none() {
            // Jev 离线：直接第三级（无 LLM 则进桶）
            return self
                .third_level(question, answer, Provisional::new(provisional, max))
                .await;
        }
        match self.jev_multi_items(question, selected).await {
            Ok(judge) => {
                let mut score = provisional;
                let mut undecided = false;
                for item in &judge.items {
                    match self.downgrade_route(route_from_margin(
                        item.margin,
                        thresholds.accept,
                        thresholds.reject,
                    )) {
                        Route::AutoAccept => {}            // 漏选确认必错：分不加
                        Route::AutoReject => score += 1.0, // Jev 否定题面答案项：视为已覆盖
                        Route::LlmReview => undecided = true,
                        Route::HumanReview => {
                            return pending_grade(
                                provisional,
                                max,
                                vec![],
                                judge.error_tags.clone(),
                            );
                        }
                    }
                }
                if undecided {
                    return self
                        .third_level(
                            question,
                            answer,
                            Provisional {
                                score,
                                max,
                                items: vec![],
                                error_tags: judge.error_tags,
                            },
                        )
                        .await;
                }
                let complete = score >= max;
                GradeResult {
                    verdict: if complete {
                        Verdict::Acceptable
                    } else {
                        Verdict::Partial
                    },
                    score: score.min(max),
                    max,
                    confidence: min_margin(&judge.items),
                    graded_by: GradedBy::Jev,
                    items: vec![],
                    error_tags: if complete { vec![] } else { judge.error_tags },
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "jev_multi_items failed, falling back to third level");
                self.third_level(question, answer, Provisional::new(provisional, max))
                    .await
            }
        }
    }

    /// 规则已判路径：正确 → 直接完（0 成本）；错误 → 错因标签（选择题）/逐空升级（填空）
    async fn grade_after_rule(
        &self,
        question: &Question,
        answer: &Answer,
        result: GradeResult,
    ) -> GradeResult {
        if matches!(result.verdict, Verdict::Correct | Verdict::Acceptable) {
            return result;
        }
        if self.jev.is_none() {
            return result; // Jev 离线：规则结果无错因标签（离线矩阵）
        }
        if matches!(question.kind, QuestionKind::FillBlank(_)) {
            self.grade_fill_jev(question, answer, result).await
        } else {
            self.apply_error_tags(question, answer, result).await
        }
    }

    /// 选择题错因：单请求 error_cause（标签失败只丢标签，不丢规则结果）
    async fn apply_error_tags(
        &self,
        question: &Question,
        answer: &Answer,
        mut result: GradeResult,
    ) -> GradeResult {
        let Some(jev) = &self.jev else {
            return result; // 调用方已挡；防御性保留（独立调用不 panic）
        };
        let state = answer_state(question, answer);
        let questions = error_cause_questions(&question.lang.0, vec![]);
        match jev.evaluate(state, &questions).await {
            Ok(answers) => {
                result.error_tags = tag_error_cause(&answers, self.thresholds.accept);
            }
            Err(err) => {
                tracing::warn!(error = %err, "error_cause tagging failed, keeping rule result");
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{BlankItem, ChoiceBody, FillBody, OptionItem, Provenance};
    use std::sync::Arc;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn choice_question() -> Question {
        Question::new(
            "She is quick.".into(),
            QuestionKind::SingleChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "quick".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "slowly".into(),
                    },
                ],
                answer: vec![0],
            }),
            2,
            vec!["vocab".into()],
            Provenance::Seed,
        )
    }

    fn multi_question() -> Question {
        Question::new(
            "Pick two.".into(),
            QuestionKind::MultiChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "alpha".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "beta".into(),
                    },
                    OptionItem {
                        id: 2,
                        text: "gamma".into(),
                    },
                ],
                answer: vec![0, 2],
            }),
            2,
            vec![],
            Provenance::Seed,
        )
    }

    fn fill_question() -> Question {
        Question::new(
            "She ___ a student.".into(),
            QuestionKind::FillBlank(FillBody {
                sentence: "She ___ a student.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["is".into(), "'s".into()],
                }],
            }),
            2,
            vec!["grammar".into()],
            Provenance::Seed,
        )
    }

    async fn client_with(server: &MockServer) -> JevClient {
        JevClient::with_options(
            &format!("{}/v1", server.uri()),
            ambition_jev::DEFAULT_JEV_MODEL,
            Some("k".into()),
            None,
        )
    }

    fn offline_router() -> Grader {
        Grader::offline()
    }

    #[tokio::test]
    async fn test_grade_rule_decides_without_ai_at_full_confidence() {
        let grader = offline_router();
        let grade = grader
            .grade(&choice_question(), &Answer::Choice(vec![0]))
            .await;
        assert_eq!(grade.verdict, Verdict::Correct);
        assert_eq!(grade.graded_by, GradedBy::Rule);
        assert_eq!(grade.confidence, 1.0, "规则确定性判定");

        let wrong = grader
            .grade(&choice_question(), &Answer::Choice(vec![1]))
            .await;
        assert_eq!(wrong.verdict, Verdict::Wrong);
        assert!(
            wrong.error_tags.is_empty(),
            "Jev 离线：规则结果无错因标签（离线矩阵）"
        );
    }

    #[tokio::test]
    async fn test_grade_rule_gap_offline_pushes_pending() {
        let grader = offline_router();
        let grade = grader
            .grade(&multi_question(), &Answer::Choice(vec![0]))
            .await;
        assert_eq!(grade.verdict, Verdict::Pending, "漏选离线 → 进桶");
        assert_eq!(grade.confidence, 0.0);
        assert_eq!((grade.score, grade.max), (1.0, 2.0), "暂定分 = 已选数");
    }

    #[tokio::test]
    async fn test_grade_rule_gap_jev_partial_and_confidence_routing() {
        let server = MockServer::start().await;
        // 漏选 option 2：高置信属于正确集 → Partial
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(body_string_contains("belongs_2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "belongs_2": {"type": "noul", "noul": 0.9},
                    "error_cause": {"type": "choice", "choice": "careless",
                                    "probabilities": {"careless": 0.8}, "confidence": 0.9}
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
            .grade(&multi_question(), &Answer::Choice(vec![0]))
            .await;
        assert_eq!(grade.verdict, Verdict::Partial, "确认漏选 → 部分分");
        assert_eq!((grade.score, grade.max), (1.0, 2.0));
        assert_eq!(grade.graded_by, GradedBy::Jev);
        assert!((grade.confidence - 0.9).abs() < 1e-5, "整体置信 = 最弱判断");
        assert_eq!(grade.error_tags, vec![ErrorTag::Careless]);
    }

    #[tokio::test]
    async fn test_grade_rule_gap_gray_band_goes_pending_without_llm() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {"belongs_2": {"type": "noul", "noul": 0.5}}
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let grade = grader
            .grade(&multi_question(), &Answer::Choice(vec![0]))
            .await;
        assert_eq!(grade.verdict, Verdict::Pending, "灰区无 LLM → 桶");
        assert_eq!(grade.confidence, 0.0);
    }

    #[tokio::test]
    async fn test_grade_rule_gap_jev_disputes_key_to_acceptable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "belongs_2": {"type": "noul", "noul": 0.05},
                    "error_cause": {"type": "choice", "choice": "no_error",
                                    "probabilities": {"no_error": 0.9}, "confidence": 0.9}
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
            .grade(&multi_question(), &Answer::Choice(vec![0]))
            .await;
        assert_eq!(
            grade.verdict,
            Verdict::Acceptable,
            "题面答案项被否定 → 作答完整"
        );
        assert_eq!((grade.score, grade.max), (2.0, 2.0));
        assert!(grade.error_tags.is_empty(), "可接受答案不带错因");
    }

    #[tokio::test]
    async fn test_apply_error_tags_on_wrong_choice_keeps_rule_verdict() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {"error_cause": {"type": "choice", "choice": "word_sense",
                            "probabilities": {"word_sense": 0.95}, "confidence": 0.93}}
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let grade = grader
            .grade(&choice_question(), &Answer::Choice(vec![1]))
            .await;
        assert_eq!(grade.verdict, Verdict::Wrong, "判定权仍在规则");
        assert_eq!(grade.graded_by, GradedBy::Rule);
        assert_eq!(grade.error_tags, vec![ErrorTag::WordSense], "错因来自 Jev");
        // 正确作答 0 成本：不发请求（mock 只挂了 1 次未声明次数上限，用正确路径无错因佐证）
        let correct = grader
            .grade(&choice_question(), &Answer::Choice(vec![0]))
            .await;
        assert!(correct.error_tags.is_empty());
        assert_eq!(correct.graded_by, GradedBy::Rule);
    }

    #[tokio::test]
    async fn test_grade_after_rule_keeps_rule_result_when_tagging_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(422))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let grade = grader
            .grade(&choice_question(), &Answer::Choice(vec![1]))
            .await;
        assert_eq!(grade.verdict, Verdict::Wrong, "标签失败不吞规则结果（P-8）");
        assert!(grade.error_tags.is_empty());
    }

    #[tokio::test]
    async fn test_grade_fill_offline_keeps_rule_result() {
        let grader = offline_router();
        let grade = grader
            .grade(&fill_question(), &Answer::Fill(vec!["was".to_string()]))
            .await;
        assert_eq!(grade.verdict, Verdict::Wrong, "离线：规则结果直出");
        assert_eq!(grade.graded_by, GradedBy::Rule);
        assert_eq!(grade.score, 0.0);
    }
}
