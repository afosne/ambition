//! 翻译题判分（M5-W2，docs/07 §1 Translation 行）：无规则层 → `translation4` rubric composite。
//! 形态不匹配/rubric 缺失/无 Jev → Pending 进桶（永不 Err、不伪造判分；LLM 不打分）。

use crate::corpus::rubric::load_rubric;
use crate::*;

/// 考试翻译默认 rubric（M5-W2；题面 rubric_id 缺省时按 kind 取内置，docs/07 §3）
pub const TRANSLATION_RUBRIC: &str = "translation4";

impl Grader {
    /// 翻译题判分（M5-W2，docs/07 §1 Translation 行：规则层空 → Jev composite 4 维 + veto；
    /// 形态不匹配/rubric 缺失/无 Jev → Pending 进桶，永不 Err、不伪造判分）
    pub(crate) async fn grade_translation_question(
        &self,
        question: &Question,
        body: &TranslationBody,
        answer: &Answer,
    ) -> GradeResult {
        let Answer::Text(typed) = answer else {
            tracing::warn!(qid = %question.qid.0, "translation expects Answer::Text; pending");
            return pending_grade(0.0, 1.0, vec![], vec![]);
        };
        let Some(rubric) = load_rubric(TRANSLATION_RUBRIC) else {
            tracing::warn!("translation rubric asset missing; pending");
            return pending_grade(0.0, 100.0, vec![], vec![]);
        };
        // state 只进 l2（docs/05 §3.5）：reference = 目标语参考译文；源文（l1）只在题面展示
        let state = JevState(serde_json::json!({
            "reference": &body.reference,
            "student_answer": typed,
        }));
        self.grade_text_composite(state, rubric, &question.lang.0)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::client_with;

    fn translation_question(reference: &str) -> Question {
        Question {
            qid: ambition_core::Qid("t-trans:1".into()),
            kind: QuestionKind::Translation(ambition_core::TranslationBody {
                direction: ambition_core::TranslationDirection {
                    from: ambition_core::LangTag("zh".into()),
                    to: ambition_core::LangTag("en".into()),
                },
                source_text: "我喜欢春天。".into(),
                reference: reference.into(),
            }),
            stem: "Translate the following sentence into English.".into(),
            difficulty: 3,
            tags: vec!["translation".to_string(), "cet4".to_string()],
            provenance: ambition_core::Provenance::Seed,
            quality_score: 1.0,
            lang: ambition_core::LangTag("en".into()),
        }
    }

    #[tokio::test]
    async fn test_grade_translation_question_scores_composite_and_tags_veto() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "accuracy": {"type": "score", "score": 2.0, "confidence": 0.9},
                    "completeness": {"type": "score", "score": 2.0, "confidence": 0.85},
                    "fluency": {"type": "score", "score": 1.0, "confidence": 0.8},
                    "naturalness": {"type": "score", "score": 1.0, "confidence": 0.75},
                    "veto_meaning_changed": {"type": "noul", "noul": 0.9},
                    "veto_heavy_chinglish": {"type": "noul", "noul": 0.4}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(std::sync::Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let question = translation_question("I like spring.");
        let QuestionKind::Translation(body) = &question.kind else {
            panic!("翻译题构造错误");
        };
        let grade = grader
            .grade_translation_question(&question, body, &Answer::Text("I like spring.".into()))
            .await;
        // 加权：0.4×1 + 0.25×1 + 0.2×0.5 + 0.15×0.5 = 0.825 → ×100 = 82.5 → 档 80；heavy_chinglish 触发 → 降档 70
        assert_eq!(
            (grade.score, grade.max),
            (70.0, 100.0),
            "80 档 + veto 降一档 = 70"
        );
        assert_eq!(grade.verdict, Verdict::Correct, "70/100 ≥ 60% → Correct");
        assert_eq!(grade.graded_by, GradedBy::Jev);
        assert!(
            matches!(grade.detail, GradeDetail::Dims(_)),
            "detail 载维度分"
        );
        assert_eq!(
            grade.error_tags,
            vec![ErrorTag::Chinglish],
            "veto → error_tag"
        );
        assert!((grade.confidence - 0.75).abs() < 1e-5, "min 置信");
    }

    #[tokio::test]
    async fn test_grade_translation_question_pending_on_wrong_shape_or_offline() {
        let offline = Grader::offline();
        let question = translation_question("ref");
        let QuestionKind::Translation(body) = &question.kind else {
            panic!("翻译题构造错误");
        };
        // 形态不匹配 → Pending
        let wrong_shape = offline
            .grade_translation_question(&question, body, &Answer::Choice(vec![0]))
            .await;
        assert_eq!(wrong_shape.verdict, Verdict::Pending, "非 Text 作答 → 桶");
        // 无 Jev → Pending（LLM 不打分）
        let no_jev = offline
            .grade_translation_question(&question, body, &Answer::Text("x".into()))
            .await;
        assert_eq!(no_jev.verdict, Verdict::Pending, "离线 → 桶不伪造分");
        assert_eq!(no_jev.score, 0.0);
    }
}
