//! 三级仲裁：third_level/llm_arbitrate + 仲裁载荷构造（灰区→LLM 结构化判，docs/07 §4）

use super::*;

/// LLM 复核的中性置信（无概率口径；低于 Jev 高置信路径，docs/07 §4）
pub const LLM_JUDGE_CONFIDENCE: f32 = 0.5;

impl Grader {
    /// 第三级：LLM 粗判（判断不打分）；不可用/失败 → 待裁定桶（docs/07 §4）
    pub(crate) async fn third_level(
        &self,
        question: &Question,
        answer: &Answer,
        provisional: Provisional,
    ) -> GradeResult {
        let Provisional {
            score,
            max,
            items,
            error_tags,
        } = provisional;
        match self.llm_arbitrate(question, answer).await {
            Ok(arbitration) => {
                tracing::debug!(reason = %arbitration.reason, "llm arbitration decided");
                let mut error_tags = error_tags;
                if arbitration.error_tag != ErrorTag::NoError
                    && !error_tags.contains(&arbitration.error_tag)
                {
                    error_tags.push(arbitration.error_tag);
                }
                GradeResult {
                    verdict: if arbitration.acceptable {
                        Verdict::Acceptable
                    } else {
                        Verdict::Wrong
                    },
                    score: if arbitration.acceptable { max } else { score },
                    max,
                    confidence: LLM_JUDGE_CONFIDENCE,
                    graded_by: GradedBy::LlmReview,
                    items,
                    error_tags,
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "llm arbitration failed, pushing to pending bucket");
                pending_grade(score, max, items, error_tags)
            }
        }
    }

    /// LLM 粗判（只出判断：acceptable + error_tag + reason；无 key → Offline，docs/07 §4）
    pub async fn llm_arbitrate(
        &self,
        question: &Question,
        answer: &Answer,
    ) -> Result<ambition_core::Arbitration, LlmError> {
        let Some(llm) = &self.llm else {
            return Err(LlmError::Offline);
        };
        llm.structured::<ambition_core::Arbitration>(arbitrate_request(question, answer))
            .await
    }
}

/// 第三级 prompt 的 user 载荷（英文；判断所需上下文齐全）
fn arbitrate_payload(question: &Question, answer: &Answer) -> String {
    let mut lines = vec![format!("Question: {}", question.stem)];
    if let QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) = &question.kind {
        lines.push(
            body.options
                .iter()
                .map(|o| format!("  {}: {}", o.id, o.text))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let reference: Vec<&str> = body
            .options
            .iter()
            .filter(|o| body.answer.contains(&o.id))
            .map(|o| o.text.as_str())
            .collect();
        lines.push(format!("Reference: {}", reference.join(" / ")));
    }
    if let QuestionKind::FillBlank(fill) = &question.kind {
        let reference: Vec<String> = fill.blanks.iter().map(|b| b.acceptable.join("|")).collect();
        lines.push(format!("Reference (per blank): {}", reference.join(" ; ")));
    }
    lines.push(format!("Student answer: {}", answer_display(answer)));
    lines.join("\n")
}

/// 第三级请求（kind=Arbitrate；temperature 0 —— 判断不采样；schema 约束三字段）
fn arbitrate_request(question: &Question, answer: &Answer) -> GenRequest {
    let mut request = GenRequest::new(
        LlmKind::Arbitrate,
        include_str!("../../../data/prompts/arbitrate.md"),
        arbitrate_payload(question, answer),
    );
    request.schema = Some(arbitrate_schema());
    request.temperature = 0.0;
    request
}

/// Arbitration 的 JSON Schema（enum 取自 ErrorTag 全集，防幻觉键）
fn arbitrate_schema() -> serde_json::Value {
    let tags: Vec<&str> = ErrorTag::all().iter().map(|t| t.as_str()).collect();
    serde_json::json!({
        "type": "object",
        "properties": {
            "acceptable": {"type": "boolean"},
            "error_tag": {"type": "string", "enum": tags},
            "reason": {"type": "string"}
        },
        "required": ["acceptable", "error_tag", "reason"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::ERROR_TAGS_VERSION;
    use ambition_core::{BlankItem, ChoiceBody, FillBody, OptionItem, Provenance};
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
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

    fn offline_router() -> Grader {
        Grader::offline()
    }

    #[test]
    fn test_arbitrate_request_pins_schema_temperature_and_prompt() {
        let request = arbitrate_request(&choice_question(), &Answer::Choice(vec![1]));
        assert_eq!(request.kind, LlmKind::Arbitrate);
        assert_eq!(request.temperature, 0.0, "判断不采样");
        assert!(request.schema.is_some(), "schema 约束三字段");
        assert!(request.system.contains("never score"), "内嵌 prompt 生效");
        assert!(request.user.contains("Student answer"), "载荷含作答");
        assert!(ERROR_TAGS_VERSION.starts_with('1'), "语料版本随判分口径");
    }

    #[test]
    fn test_arbitrate_payload_carries_reference_and_student_answer() {
        let choice_payload = arbitrate_payload(&choice_question(), &Answer::Choice(vec![1]));
        assert!(choice_payload.starts_with("Question:"), "题干开头");
        assert!(choice_payload.contains("Reference: quick"), "参考答案");
        assert!(choice_payload.contains("Student answer: 1"), "学生作答");
        assert!(choice_payload.contains("  1: slowly"), "选项表");
        let fill_payload = arbitrate_payload(&fill_question(), &Answer::Fill(vec!["was".into()]));
        assert!(
            fill_payload.contains("Reference (per blank): is|'s"),
            "逐空可接受表: {fill_payload}"
        );
        assert!(fill_payload.contains("Student answer: was"), "填空作答");
        assert!(
            !fill_payload
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "载荷纯 l2"
        );
    }

    #[test]
    fn test_arbitrate_schema_enum_covers_all_tags() {
        let schema = arbitrate_schema();
        let tags = schema["properties"]["error_tag"]["enum"]
            .as_array()
            .expect("enum 数组");
        assert_eq!(tags.len(), 28, "28 类全集防幻觉");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(answer_display(&Answer::Choice(vec![0, 2])), "0,2");
        assert_eq!(
            answer_display(&Answer::Fill(vec!["a".into(), "b".into()])),
            "a ; b"
        );
    }

    #[tokio::test]
    async fn test_llm_arbitrate_parses_structured_judgment() {
        use ambition_llm::{LlmKind, OpenAiCompat};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": serde_json::json!({
                    "acceptable": true,
                    "error_tag": "collocation",
                    "reason": "variant fits the blank"
                }).to_string()}}],
                "usage": {"prompt_tokens": 20, "completion_tokens": 10}
            })))
            .mount(&server)
            .await;
        let router = ambition_llm::LlmRouter::new(
            Box::new(OpenAiCompat::new(
                format!("{}/v1", server.uri()),
                "test-model",
                Some("k".into()),
            )),
            None,
            Box::new(ambition_llm::MemLedger::new()),
        );
        let grader = Grader::new(None, Some(Arc::new(router)), Thresholds::english());
        let arbitration = grader
            .llm_arbitrate(&fill_question(), &Answer::Fill(vec!["is".into()]))
            .await
            .expect("复核成功");
        assert!(arbitration.acceptable);
        assert_eq!(arbitration.error_tag, ErrorTag::Collocation);
        assert_eq!(arbitration.reason, "variant fits the blank");
        assert_eq!(
            arbitrate_request(&fill_question(), &Answer::Fill(vec![])).kind,
            LlmKind::Arbitrate
        );
    }

    #[tokio::test]
    async fn test_third_level_without_llm_pushes_pending() {
        let grader = offline_router();
        let grade = grader
            .third_level(
                &multi_question(),
                &Answer::Choice(vec![0]),
                Provisional {
                    score: 1.0,
                    max: 2.0,
                    items: vec![],
                    error_tags: vec![ErrorTag::Careless],
                },
            )
            .await;
        assert_eq!(grade.verdict, Verdict::Pending);
        assert_eq!(grade.confidence, 0.0);
        assert_eq!(grade.error_tags, vec![ErrorTag::Careless], "标签随桶保留");
    }

    #[tokio::test]
    async fn test_third_level_llm_failure_is_visible_pending() {
        use ambition_llm::OpenAiCompat;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;
        let router = ambition_llm::LlmRouter::new(
            Box::new(OpenAiCompat::new(
                format!("{}/v1", server.uri()),
                "m",
                Some("k".into()),
            )),
            None,
            Box::new(ambition_llm::MemLedger::new()),
        );
        let grader = Grader::new(None, Some(Arc::new(router)), Thresholds::english());
        let grade = grader
            .third_level(
                &choice_question(),
                &Answer::Choice(vec![1]),
                Provisional::new(0.0, 1.0),
            )
            .await;
        assert_eq!(
            grade.verdict,
            Verdict::Pending,
            "复核失败进桶不静默（warn）"
        );
        assert_eq!(grade.graded_by, GradedBy::Rule);
    }
}
