//! rule 判定环测试（从 rule.rs 拆出，docs/01 §组织：文件 ≤500 行；P-16 测试随被测物走）

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::common::fixtures::*;
    use ambition_core::{BlankItem, Provenance};
    use std::sync::Arc;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    #[tokio::test]
    async fn test_grade_composite_mismatched_answer_is_pending() {
        // grade_composite 配对：作答位数与子题数不齐 → 防御性 Pending（不伪造判分）
        let grader = offline_router();
        let q = Question::new(
            "I ___ a.".into(),
            QuestionKind::Cloze(ambition_core::ClozeBody {
                passage: "I ___ a.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["am".into()],
                }],
            }),
            1,
            vec![],
            Provenance::Seed,
        );
        let grade = grader.grade(&q, &Answer::Choice(vec![0])).await;
        assert_eq!(grade.verdict, Verdict::Pending, "形态不匹配 → Pending");
        assert_eq!(grade.max, 1.0, "max = 子题数");
    }
}
