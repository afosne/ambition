//! 作文题判分（M5-W3，docs/07 §1 Essay 行）：词数硬校验（MVP=空卷 Pending）→ `rubric_id` rubric composite。
//! 中文题面 `prompt_zh` 永不进 state（CJK 低准确率红线 docs/05 §3.5）——state = {essay, word_count, genre}；
//! 卷面级字数/时长红线（字数红线/超时/禁粘贴）在卷面模拟层（docs/10 §5，W8）与套卷 run 层（W6），本层不重复扣分。

use crate::corpus::rubric::load_rubric;
use crate::*;

impl Grader {
    /// 作文判分（永不 Err、不伪造判分：形态不匹配/rubric 缺失/空卷 → Pending 进桶）
    pub(crate) async fn grade_essay_question(
        &self,
        question: &Question,
        body: &EssayPrompt,
        answer: &Answer,
    ) -> GradeResult {
        let Answer::Text(text) = answer else {
            tracing::warn!(qid = %question.qid.0, "essay expects Answer::Text; pending");
            return pending_grade(0.0, 1.0, vec![], vec![]);
        };
        let Some(rubric) = load_rubric(&body.rubric_id) else {
            tracing::warn!(id = %body.rubric_id, "essay rubric asset missing; pending");
            return pending_grade(0.0, 1.0, vec![], vec![]);
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            // 词数硬校验（docs/07 §1 Essay ①规则层）：空卷不送 Jev → 桶人工
            tracing::warn!(qid = %question.qid.0, "empty essay; pending");
            return pending_grade(0.0, rubric.max, vec![], vec![]);
        }
        let word_count = trimmed.split_whitespace().count();
        // state 只进英文内容（l2/中性标签）；genre = 英文体裁标签
        let state = JevState(serde_json::json!({
            "essay": trimmed,
            "word_count": word_count,
            "genre": body.genre,
        }));
        self.grade_text_composite(state, rubric, &question.lang.0)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::client_with;
    use crate::corpus::rubric::load_rubric;

    fn essay_question(rubric_id: &str, track: &str) -> Question {
        Question {
            qid: ambition_core::Qid(format!("t-essay:{rubric_id}:{track}")),
            kind: QuestionKind::Essay(EssayPrompt {
                prompt_zh: "以“绿色校园”为题写一篇短文。".into(),
                genre: "argument".into(),
                track: track.into(),
                rubric_id: rubric_id.into(),
                word_range: (120, 180),
                minutes: 30,
            }),
            stem: "Write an essay according to the prompt.".into(),
            difficulty: 3,
            tags: vec!["essay".to_string(), track.to_string()],
            provenance: ambition_core::Provenance::Seed,
            quality_score: 1.0,
            lang: ambition_core::LangTag("en".into()),
        }
    }

    #[tokio::test]
    async fn test_grade_essay_question_scores_gaokao_rubric_composite() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "task": {"type": "score", "score": 2.0, "confidence": 0.9},
                    "structure": {"type": "score", "score": 2.0, "confidence": 0.85},
                    "grammar": {"type": "score", "score": 1.0, "confidence": 0.8},
                    "lexicon": {"type": "score", "score": 1.0, "confidence": 0.75},
                    "coherence": {"type": "score", "score": 2.0, "confidence": 0.9},
                    "format": {"type": "score", "score": 2.0, "confidence": 0.95},
                    "veto_off_topic": {"type": "noul", "noul": 0.9},
                    "veto_meaning_changed": {"type": "noul", "noul": 0.8},
                    "veto_heavy_chinglish": {"type": "noul", "noul": 0.9}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(std::sync::Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let question = essay_question("gaokao25", "cet4");
        let QuestionKind::Essay(body) = &question.kind else {
            panic!("作文题构造错误");
        };
        let answer = Answer::Text(
            "Nowadays many students spend too much time on phones. In my opinion, we should use them wisely and keep a balance between study and relaxation. Only in this way can we focus on our lessons.".into(),
        );
        let grade = grader.grade_essay_question(&question, body, &answer).await;
        // 混合分（3 档 levels 刻度 0..2）：0.25×1+0.2×1+0.2×0.5+0.15×0.5+0.1×1+0.1×1 = 0.825 → ×25 = 20.625 → 档 20；无 veto
        assert_eq!((grade.score, grade.max), (20.0, 25.0), "23.75 → 下档 20");
        assert_eq!(grade.verdict, Verdict::Correct, "20/25 ≥ 60%");
        assert!(
            matches!(grade.detail, GradeDetail::Dims(_)),
            "detail 载 6 维"
        );
        assert!(grade.error_tags.is_empty(), "无 veto 无错因");
        assert!(
            (grade.confidence - 0.75).abs() < 1e-5,
            "min 置信 = lexicon 0.75"
        );
    }

    #[tokio::test]
    async fn test_grade_essay_question_pending_on_empty_or_missing_rubric() {
        let offline = Grader::offline();
        // 空卷 → Pending（词数硬校验）
        let question = essay_question("gaokao25", "cet4");
        let QuestionKind::Essay(body) = &question.kind else {
            panic!("作文题构造错误");
        };
        let empty = offline
            .grade_essay_question(&question, body, &Answer::Text("   ".into()))
            .await;
        assert_eq!(empty.verdict, Verdict::Pending, "空卷 → 桶");
        // rubric 资产缺失 → Pending（不臆造口径）
        let bad = essay_question("no-such-rubric", "cet4");
        let QuestionKind::Essay(bad_body) = &bad.kind else {
            panic!("作文题构造错误");
        };
        let missing = offline
            .grade_essay_question(&bad, bad_body, &Answer::Text("Hello world essay.".into()))
            .await;
        assert_eq!(missing.verdict, Verdict::Pending, "缺 rubric → 桶");
        // 形态不匹配 → Pending
        let wrong = offline
            .grade_essay_question(&question, body, &Answer::Choice(vec![0]))
            .await;
        assert_eq!(wrong.verdict, Verdict::Pending, "非 Text 作答 → 桶");
    }

    #[tokio::test]
    async fn test_grade_essay_uses_ielts_rubric_for_ielts_track() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "task_response": {"type": "score", "score": 2.0, "confidence": 0.9},
                    "coherence_cohesion": {"type": "score", "score": 2.0, "confidence": 0.9},
                    "lexical_resource": {"type": "score", "score": 1.0, "confidence": 0.9},
                    "grammatical_range": {"type": "score", "score": 1.0, "confidence": 0.9},
                    "veto_off_topic": {"type": "noul", "noul": 0.9},
                    "veto_meaning_changed": {"type": "noul", "noul": 0.9},
                    "veto_heavy_chinglish": {"type": "noul", "noul": 0.9}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(std::sync::Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let question = essay_question("ielts_task2", "ielts");
        let QuestionKind::Essay(body) = &question.kind else {
            panic!("作文题构造错误");
        };
        assert_eq!(
            load_rubric(&body.rubric_id).expect("内置雅思 rubric").max,
            9.0
        );
        let grade = grader
            .grade_essay_question(
                &question,
                body,
                &Answer::Text("Some people think that... I strongly agree with this view for two reasons. First,... Second,... In conclusion,...".into()),
            )
            .await;
        // 两维满分两维中档（刻度 0..2）：0.25×1+0.25×1+0.25×0.5+0.25×0.5 = 0.75 → ×9 = 6.75 → 档 6；无 veto
        assert_eq!((grade.score, grade.max), (6.0, 9.0), "6.0 → band 6");
        assert_eq!(grade.verdict, Verdict::Correct, "6/9 ≥ 60%");
    }
}
