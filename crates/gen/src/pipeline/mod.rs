//! 生成编排（细分域根，docs/06 §4）：Generator/new/generate/qc_question/build_request + 重生成上限
//! 子组件 `pipeline::{spec,prompt,qc}`；跨域引用一律 `use crate::*`（docs/01 §1）。

pub mod prompt;
pub mod qc;
pub mod spec;

pub use {qc::*, spec::*};

use self::prompt::{draft_schema, render_prompt};
use crate::*;

/// 重生成轮数上限（docs/06 §1：质检回路 ≤2 轮）
pub const MAX_REGEN_ROUNDS: u32 = 2;

/// 生成器（llm 路由 + jev 质检）
pub struct Generator {
    llm: LlmRouter,
    jev: JevClient,
}

impl Generator {
    /// 构造（两依赖注入，engine 按 env/config 装配）
    pub fn new(llm: LlmRouter, jev: JevClient) -> Self {
        Self { llm, jev }
    }

    /// 主流程：结构化出题 + 质检回路（重生成 ≤2 轮，仍不合格丢弃并 warn）
    pub async fn generate(&self, spec: &GenSpec) -> Result<Vec<ambition_core::Question>, GenError> {
        if spec.kinds.is_empty() {
            return Err(GenError::InvalidSpec("kinds empty".into()));
        }
        let mut accepted: Vec<ambition_core::Question> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut failures: Vec<String> = Vec::new();

        for round in 0..=MAX_REGEN_ROUNDS {
            let needed = spec.count.saturating_sub(accepted.len() as u32);
            if needed == 0 {
                break;
            }
            let req = self.build_request(spec, needed, &failures);
            let batch: DraftBatch = self.llm.structured(req).await?;
            let mut next_failures = Vec::new();
            for draft in &batch.questions {
                let question = match draft.to_question() {
                    Ok(q) => q,
                    Err(err) => {
                        tracing::warn!(round, error = %err, "draft invalid, dropping");
                        next_failures.push(format!("invalid draft: {err}"));
                        continue;
                    }
                };
                if !seen.insert(question.qid.0.clone()) {
                    continue; // 跨轮重复内容去重
                }
                match self.qc_question(&question).await {
                    Ok(report) if report.passed => {
                        let mut q = question;
                        q.quality_score = report.quality_score;
                        accepted.push(q);
                        if accepted.len() as u32 >= spec.count {
                            break;
                        }
                    }
                    Ok(report) => {
                        tracing::warn!(
                            round,
                            qid = %question.qid.0,
                            quality = report.quality_score,
                            "qc rejected, will regenerate"
                        );
                        next_failures.push(format!(
                            "qc failed (key={:.2}, natural={:.2}) for stem: {}",
                            report.key_correct, report.natural, question.stem
                        ));
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
            failures = next_failures;
            if failures.is_empty() {
                break;
            }
        }

        if !failures.is_empty() {
            tracing::warn!(
                rounds = MAX_REGEN_ROUNDS,
                dropped = failures.len(),
                "dropping questions still failing after regeneration limit"
            );
        }
        if accepted.is_empty() && spec.count > 0 {
            return Err(GenError::AllRejected {
                rounds: MAX_REGEN_ROUNDS,
            });
        }
        Ok(accepted)
    }

    /// 单题质检：一次 Jev 请求 fan-out 全部检查（state 只含题干/选项/答案，l2 only）
    pub async fn qc_question(
        &self,
        question: &ambition_core::Question,
    ) -> Result<QcReport, GenError> {
        let kind_tag = match question.kind {
            ambition_core::QuestionKind::SingleChoice(_) => QuestionKindTag::SingleChoice,
            ambition_core::QuestionKind::MultiChoice(_) => QuestionKindTag::MultiChoice,
            ambition_core::QuestionKind::FillBlank(_) => QuestionKindTag::FillBlank,
            ambition_core::QuestionKind::Cloze(_) => QuestionKindTag::Cloze,
            ambition_core::QuestionKind::Reading(_) => QuestionKindTag::Reading,
            ambition_core::QuestionKind::SevenOfFive(_) => QuestionKindTag::SevenOfFive,
            ambition_core::QuestionKind::GrammarFill(_) => QuestionKindTag::GrammarFill,
        };
        let state = qc_state(question, kind_tag);
        let questions = qc_questions(kind_tag);
        let answers = self.jev.evaluate(state, &questions).await?;
        Ok(QcReport::from_answers(&question.kind, &answers)?)
    }

    /// 组生成请求（prompt 渲染 + JSON Schema + 上轮失败反馈）
    fn build_request(&self, spec: &GenSpec, needed: u32, failures: &[String]) -> GenRequest {
        let mut req = GenRequest::new(
            LlmKind::QuestionGen,
            "You are a precise exam item writer.".to_string(),
            render_prompt(spec, needed, failures),
        );
        req.schema = Some(draft_schema());
        req
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use ambition_core::Provenance;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn choice_draft_json() -> serde_json::Value {
        serde_json::json!({
            "questions": [{
                "kind": "single_choice",
                "stem": "She ___ to school every day.",
                "options": ["go", "goes", "going", "gone"],
                "answers": ["goes"],
                "difficulty": 2,
                "tags": ["grammar", "sva"]
            }]
        })
    }

    fn qc_pass_body() -> serde_json::Value {
        serde_json::json!({
            "model": "jev-1.13.0",
            "answers": {
                "key_correct": {"type": "noul", "noul": 0.95},
                "natural": {"type": "score", "score": 1.9, "confidence": 0.9},
                "difficulty_fit": {"type": "score", "score": 2.0, "confidence": 0.8},
                "unique_answer": {"type": "noul", "noul": 0.92},
                "distractor_valid": {"type": "noul", "noul": 0.85}
            },
            "usage": {"input_tokens": 200, "output_tokens": 30}
        })
    }

    fn qc_fail_body() -> serde_json::Value {
        serde_json::json!({
            "model": "jev-1.13.0",
            "answers": {
                "key_correct": {"type": "noul", "noul": 0.4},
                "natural": {"type": "score", "score": 0.5, "confidence": 0.9},
                "difficulty_fit": {"type": "score", "score": 1.0, "confidence": 0.8},
                "unique_answer": {"type": "noul", "noul": 0.3},
                "distractor_valid": {"type": "noul", "noul": 0.6}
            },
            "usage": {"input_tokens": 200, "output_tokens": 30}
        })
    }

    async fn generator_with(
        llm_server: &MockServer,
        jev_server: &MockServer,
        llm_key: Option<String>,
        jev_key: Option<String>,
    ) -> Generator {
        let openai = ambition_llm::OpenAiCompat::new(
            format!("{}/v1", llm_server.uri()),
            "test-model",
            llm_key,
        );
        let router = LlmRouter::new(
            Box::new(openai),
            None,
            Box::new(ambition_llm::MemLedger::new()),
        );
        let jev = JevClient::with_options(&jev_server.uri(), "jev-latest", jev_key, None);
        Generator::new(router, jev)
    }

    #[tokio::test]
    async fn test_build_request_carries_schema_and_needed_count() {
        let openai = ambition_llm::OpenAiCompat::new("http://x/v1", "m", None);
        let router = LlmRouter::new(
            Box::new(openai),
            None,
            Box::new(ambition_llm::MemLedger::new()),
        );
        let generator = Generator::new(router, JevClient::new(None));
        let req = generator.build_request(&spec(), 2, &["bad stem".into()]);
        assert_eq!(req.kind, LlmKind::QuestionGen);
        assert!(req.schema.is_some(), "结构化输出 schema 在场");
        assert!(
            req.user.contains("exactly 2"),
            "按 needed 出题数: {}",
            req.user
        );
        assert!(req.user.contains("Previous attempts"), "失败反馈注入");
        assert!((req.temperature - 0.7).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_qc_question_fans_out_single_jev_request() {
        let llm_server = MockServer::start().await;
        let jev_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(qc_pass_body()))
            .expect(1)
            .mount(&jev_server)
            .await;
        let generator =
            generator_with(&llm_server, &jev_server, Some("k".into()), Some("j".into())).await;
        let draft = DraftQuestion {
            kind: QuestionKindTag::SingleChoice,
            stem: "She ___ daily.".into(),
            options: vec!["run".into(), "runs".into()],
            answers: vec!["runs".into()],
            blanks: vec![],
            passage: String::new(),
            items: vec![],
            difficulty: 2,
            tags: vec![],
        };
        let q = draft.to_question().expect("题");
        let report = generator.qc_question(&q).await.expect("质检");
        assert!(report.passed, "mock 返回全过: {report:?}");
        assert_eq!(report.unique_answer, Some(0.92));
    }

    #[tokio::test]
    async fn test_new_generator_stores_dependencies_and_qc_offline() {
        let openai = ambition_llm::OpenAiCompat::new("http://x/v1", "m", None);
        let router = LlmRouter::new(
            Box::new(openai),
            None,
            Box::new(ambition_llm::MemLedger::new()),
        );
        let jev = JevClient::new(None);
        let generator = Generator::new(router, jev);
        let fill = DraftQuestion {
            kind: QuestionKindTag::FillBlank,
            stem: "I ___.".into(),
            options: vec![],
            answers: vec![],
            blanks: vec![vec!["go".into()]],
            passage: String::new(),
            items: vec![],
            difficulty: 1,
            tags: vec![],
        }
        .to_question()
        .expect("题");
        // 依赖注入后对象可用；无 key → 显式 Offline（不静默降级，P-8）
        let result = generator.qc_question(&fill).await;
        assert!(
            matches!(result, Err(GenError::Jev(JevError::Offline))),
            "无 key → Jev Offline: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_generate_end_to_end_with_mock_llm_and_jev() {
        let llm_server = MockServer::start().await;
        let jev_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains("single_choice"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": serde_json::to_string(&choice_draft_json()).expect("json")}}],
                "usage": {"prompt_tokens": 50, "completion_tokens": 40}
            })))
            .mount(&llm_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(qc_pass_body()))
            .mount(&jev_server)
            .await;

        let generator =
            generator_with(&llm_server, &jev_server, Some("k".into()), Some("j".into())).await;
        let s = GenSpec::new("cet4", vec![QuestionKindTag::SingleChoice], 1);
        let questions = generator.generate(&s).await.expect("端到端生成");
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].provenance, Provenance::Generated);
        assert!(
            (questions[0].quality_score - 1.0).abs() < 0.15,
            "qc pass → quality_score 高: {}",
            questions[0].quality_score
        );
    }

    #[tokio::test]
    async fn test_generate_rejects_all_after_regen_limit() {
        let llm_server = MockServer::start().await;
        let jev_server = MockServer::start().await;
        // LLM 每轮都出题，Jev 每次都拒 → 走满重生成轮后仍不合格
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": serde_json::to_string(&choice_draft_json()).expect("json")}}],
                "usage": {"prompt_tokens": 50, "completion_tokens": 40}
            })))
            .mount(&llm_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(qc_fail_body()))
            .mount(&jev_server)
            .await;

        let generator =
            generator_with(&llm_server, &jev_server, Some("k".into()), Some("j".into())).await;
        let s = GenSpec::new("cet4", vec![QuestionKindTag::SingleChoice], 1);
        let err = generator.generate(&s).await.expect_err("全拒");
        assert!(
            matches!(err, GenError::AllRejected { .. }),
            "全不合格显式报错（P-8 不静默）: {err}"
        );
    }

    #[tokio::test]
    async fn test_generate_offline_without_llm_key_is_explicit() {
        let llm_server = MockServer::start().await;
        let jev_server = MockServer::start().await;
        let generator = generator_with(&llm_server, &jev_server, None, None).await;
        let err = generator.generate(&spec()).await.expect_err("无 key");
        assert!(matches!(err, GenError::Llm(LlmError::Offline)), "{err}");
    }

    #[tokio::test]
    async fn test_generate_spec_requires_kinds() {
        let llm_server = MockServer::start().await;
        let jev_server = MockServer::start().await;
        let generator =
            generator_with(&llm_server, &jev_server, Some("k".into()), Some("j".into())).await;
        let mut s = spec();
        s.kinds.clear();
        let err = generator.generate(&s).await.expect_err("空 kinds");
        assert!(matches!(err, GenError::InvalidSpec(_)), "{err}");
    }

    #[test]
    fn test_draft_to_question_builds_new_passage_kinds() {
        use ambition_core::QuestionKind;
        // 完形：passage + 逐空
        let cloze: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "cloze",
            "stem": "Cloze exercise.",
            "passage": "I ___ a student. She ___ daily.",
            "blanks": [["am"], ["runs"]],
            "difficulty": 3,
            "tags": ["cloze"]
        }))
        .expect("完形草稿");
        let q = cloze.to_question().expect("转换");
        assert!(matches!(q.kind, QuestionKind::Cloze(_)));
        let QuestionKind::Cloze(body) = &q.kind else {
            panic!("cloze")
        };
        assert_eq!(body.blanks.len(), 2);
        // 语法填空
        let gf: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "grammar_fill",
            "stem": "Grammar fill.",
            "passage": "She ___ to school.",
            "blanks": [["goes"]],
            "difficulty": 2,
            "tags": ["grammar"]
        }))
        .expect("语法填空草稿");
        assert!(matches!(
            gf.to_question().expect("转换").kind,
            QuestionKind::GrammarFill(_)
        ));
        // 七选五：7 候选 + 5 答案
        let s75: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "seven_of_five",
            "stem": "Seven of five.",
            "passage": "A ___ B ___ C ___ D ___ E ___ F",
            "options": ["o1", "o2", "o3", "o4", "o5", "o6", "o7"],
            "answers": ["o1", "o3", "o5", "o7", "o2"],
            "difficulty": 4,
            "tags": ["reading"]
        }))
        .expect("七选五草稿");
        let q = s75.to_question().expect("转换");
        let QuestionKind::SevenOfFive(body) = &q.kind else {
            panic!("seven_of_five")
        };
        assert_eq!(body.options.len(), 7);
        assert_eq!(body.answer, vec![0, 2, 4, 6, 1], "答案按文本映射回 id");
        // 阅读：语篇 + 子题
        let reading: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "reading",
            "stem": "Read and answer.",
            "passage": "A short passage about cats.",
            "items": [
                {"question": "What?", "options": ["a", "b"], "answers": ["b"]},
                {"question": "Why?", "options": ["x", "y", "z"], "answers": ["y"]}
            ],
            "difficulty": 3,
            "tags": ["reading"]
        }))
        .expect("阅读草稿");
        let q = reading.to_question().expect("转换");
        let QuestionKind::Reading(body) = &q.kind else {
            panic!("reading")
        };
        assert_eq!(body.items.len(), 2);
        assert_eq!(body.items[0].answer, vec![1]);
        assert_eq!(body.items[1].answer, vec![1], "三选项第二项");
        assert_eq!(body.items[1].options.len(), 3);
    }

    #[test]
    fn test_draft_rejects_malformed_passage_kinds() {
        // 完形：passage 空/空位数不符 → Err
        let no_passage = serde_json::json!({
            "kind": "cloze", "stem": "s", "passage": "", "blanks": [["a"]],
            "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(no_passage)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "无 passage → Err"
        );
        let mismatch = serde_json::json!({
            "kind": "cloze", "stem": "s", "passage": "I ___ a.", "blanks": [["a"], ["b"]],
            "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(mismatch)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "空位数不符 → Err"
        );
        // 七选五：答案不在选项 → Err
        let bad_answer = serde_json::json!({
            "kind": "seven_of_five", "stem": "s", "passage": "A ___ B ___ C ___ D ___ E ___ F",
            "options": ["o1", "o2", "o3", "o4", "o5", "o6", "o7"],
            "answers": ["o1", "o9", "o3", "o4", "o5"], "difficulty": 3, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(bad_answer)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "答案不在选项 → Err"
        );
        // 阅读：子题缺选项 → Err
        let bad_item = serde_json::json!({
            "kind": "reading", "stem": "s", "passage": "p",
            "items": [{"question": "q", "options": ["only-one"], "answers": ["only-one"]}],
            "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(bad_item)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "子题 <2 选项 → Err"
        );
    }
}
