//! 题包进出：import_seed/import_pack/export_pack/generate/add_question/chinglish（docs/03 §3）

use super::*;

impl Engine {
    /// 导入内置 seed qpack JSON（幂等：同 qid 跳过；解析失败显式报错不静默）
    pub async fn import_seed(&self, qpack_json: &str) -> Result<PackImportReport, EngineError> {
        let pack = ambition_gen::parse_qpack(qpack_json.as_bytes())?;
        self.import_pack(&pack).await
    }

    /// 导入题包（去重 + 低质拒收，docs/06 §6）
    pub async fn import_pack(
        &self,
        pack: &ambition_gen::QPack,
    ) -> Result<PackImportReport, EngineError> {
        let existing = ambition_store::list_questions(&self.pool).await?;
        let seen: std::collections::HashSet<String> =
            existing.into_iter().map(|q| q.qid.0).collect();
        let plan = ambition_gen::plan_import(pack, &seen, ambition_gen::IMPORT_QUALITY_FLOOR);
        for question in &plan.accepted {
            ambition_store::insert_question(&self.pool, question).await?;
        }
        Ok(PackImportReport {
            added: plan.accepted.len(),
            duplicates: plan.duplicates.len(),
            low_quality: plan.low_quality.len(),
        })
    }

    /// 导出题包（标签过滤：空 = 全量；docs/06 §6 export_qpack）
    pub async fn export_pack(
        &self,
        manifest: ambition_gen::Manifest,
        tags: &[String],
    ) -> Result<ambition_gen::QPack, EngineError> {
        let all = ambition_store::list_questions(&self.pool).await?;
        let filtered: Vec<Question> = if tags.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|q| tags.iter().any(|t| q.tags.contains(t)))
                .collect()
        };
        Ok(ambition_gen::build_qpack(&filtered, manifest))
    }

    /// 结构化出题（prompt → llm → 质检回路 → 入库，docs/06 §1 generate 落库段）
    pub async fn generate(
        &self,
        spec: &ambition_gen::GenSpec,
        generator: &ambition_gen::Generator,
    ) -> Result<Vec<Question>, EngineError> {
        let questions = generator.generate(spec).await?;
        for question in &questions {
            ambition_store::insert_question(&self.pool, question).await?;
        }
        Ok(questions)
    }

    /// 注入题目（手动补题入口；seed/qpack 走 import_*）
    pub async fn add_question(&self, question: &Question) -> Result<(), EngineError> {
        ambition_store::insert_question(&self.pool, question).await?;
        Ok(())
    }

    /// 中式英语改错训练题注入题库（data/chinglish；limit=0 全量；同 qid 幂等覆盖）
    pub async fn add_chinglish_drills(&self, limit: usize) -> Result<Vec<Question>, EngineError> {
        let questions = ambition_gen::drill_questions(&ambition_gen::builtin_chinglish(), limit);
        for question in &questions {
            ambition_store::insert_question(&self.pool, question).await?;
        }
        Ok(questions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use ambition_core::{ChoiceBody, OptionItem, QuestionKind};

    #[tokio::test]
    async fn test_import_pack_dedupes_and_rejects_low_quality() {
        let engine = Engine::open_memory().await.expect("打开");
        let seed_count = engine.start_practice(0).await.expect("取题").len();

        let report = engine
            .import_seed(include_str!(
                "../../../../data/seed/en/cet4/core.qpack.json"
            ))
            .await
            .expect("再导 seed");
        assert_eq!(report.added, 0);
        assert_eq!(report.duplicates, seed_count);

        // 新题（不同内容）低质 → 拒收；seed 题重复 → 去重
        let mut low = Question::new(
            "Low quality stem.".into(),
            QuestionKind::SingleChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "x".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "y".into(),
                    },
                ],
                answer: vec![0],
            }),
            1,
            vec!["vocab".into()],
            ambition_core::Provenance::Generated,
        );
        low.quality_score = 0.1;
        let seed_dup = engine
            .start_practice(1)
            .await
            .expect("取一道 seed")
            .remove(0);
        let pack =
            ambition_gen::build_qpack(&[low, seed_dup], ambition_gen::Manifest::local("t", "cet4"));
        let report2 = engine.import_pack(&pack).await.expect("导入");
        assert_eq!(report2.low_quality, 1, "低质题拒收");
        assert_eq!(report2.duplicates, 1, "已存在题去重");
        assert_eq!(report2.added, 0, "无新增");
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            seed_count,
            "库未变"
        );
    }

    #[tokio::test]
    async fn test_export_pack_filters_by_tags_and_roundtrips() {
        let engine = Engine::open_memory().await.expect("打开");
        let all = engine.start_practice(0).await.expect("取题");
        let full = engine
            .export_pack(ambition_gen::Manifest::local("exp", "cet4"), &[])
            .await
            .expect("全量导出");
        assert_eq!(full.questions.len(), all.len());

        let bytes = serde_json::to_vec(&full).expect("序列化");
        let parsed = ambition_gen::parse_qpack(&bytes).expect("解析（防篡改通过）");
        assert_eq!(parsed.questions.len(), all.len());

        // 标签过滤：不存在的标签 → 空包
        let none = engine
            .export_pack(
                ambition_gen::Manifest::local("exp", "cet4"),
                &["no-such-tag".to_string()],
            )
            .await
            .expect("过滤导出");
        assert!(none.questions.is_empty(), "标签过滤生效");
    }

    #[tokio::test]
    async fn test_add_question_is_idempotent() {
        let engine = Engine::open_memory().await.expect("打开");
        let q = choice_question();
        engine.add_question(&q).await.expect("注入");
        engine.add_question(&q).await.expect("幂等注入");
        let out = engine
            .submit_answer(&attempt_for(q.qid, vec![0], "a-ok"))
            .await
            .expect("提交");
        assert_eq!(out.grade.verdict, Verdict::Correct);
    }

    #[tokio::test]
    async fn test_generate_persists_generated_questions() {
        use ambition_gen::{GenSpec, Generator, QuestionKindTag};
        use ambition_jev::JevClient;
        use ambition_llm::{LlmRouter, MemLedger, OpenAiCompat};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let llm_server = MockServer::start().await;
        let jev_server = MockServer::start().await;
        let draft = serde_json::json!({
            "questions": [{
                "kind": "single_choice",
                "stem": "Water ___ at 100 degrees Celsius.",
                "options": ["boil", "boils", "boiling", "boiled"],
                "answers": ["boils"],
                "difficulty": 1,
                "tags": ["science"]
            }]
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": draft.to_string()}}],
                "usage": {"prompt_tokens": 40, "completion_tokens": 30}
            })))
            .mount(&llm_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "model": "jev-1.13.0",
                "answers": {
                    "key_correct": {"type": "noul", "noul": 0.97},
                    "natural": {"type": "score", "score": 1.9, "confidence": 0.9},
                    "difficulty_fit": {"type": "score", "score": 1.8, "confidence": 0.8},
                    "unique_answer": {"type": "noul", "noul": 0.93},
                    "distractor_valid": {"type": "noul", "noul": 0.88}
                },
                "usage": {"input_tokens": 180, "output_tokens": 25}
            })))
            .mount(&jev_server)
            .await;

        let router = LlmRouter::new(
            Box::new(OpenAiCompat::new(
                format!("{}/v1", llm_server.uri()),
                "test-model",
                Some("k".into()),
            )),
            None,
            Box::new(MemLedger::new()),
        );
        let generator = Generator::new(
            router,
            JevClient::with_options(&jev_server.uri(), "jev-latest", Some("j".into()), None),
        );

        let engine = Engine::open_memory().await.expect("打开");
        let before = engine.start_practice(0).await.expect("取题").len();
        let spec = GenSpec::new("cet4", vec![QuestionKindTag::SingleChoice], 1);
        let questions = engine.generate(&spec, &generator).await.expect("生成");
        assert_eq!(questions.len(), 1);
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            before + 1,
            "生成题已入库"
        );
        // 生成 Offline 错误路径：无 key 显式报错
        let router2 = LlmRouter::new(
            Box::new(OpenAiCompat::new(
                format!("{}/v1", llm_server.uri()),
                "m",
                None,
            )),
            None,
            Box::new(MemLedger::new()),
        );
        let offline = Generator::new(
            router2,
            JevClient::with_options(&jev_server.uri(), "jev-latest", None, None),
        );
        let err = engine.generate(&spec, &offline).await.expect_err("无 key");
        assert!(
            matches!(err, EngineError::Gen(_)),
            "无 key → Gen 错误: {err}"
        );
    }

    #[tokio::test]
    async fn test_import_seed_rejects_corrupt_json_explicitly() {
        // 显式错误路径（P-8：坏 seed 不静默）——直接测 import_seed 的解析失败分支
        let engine = Engine::open_memory().await.expect("打开");
        let err = engine
            .import_seed("not a qpack")
            .await
            .expect_err("坏 seed");
        assert!(
            matches!(err, EngineError::Gen(_)),
            "坏 seed → Gen 错误: {err}"
        );
    }

    #[tokio::test]
    async fn test_add_chinglish_drills_inserts_idempotently() {
        let engine = Engine::open_memory().await.expect("打开");
        let before = engine.start_practice(0).await.expect("取题").len();
        let three = engine.add_chinglish_drills(3).await.expect("注入");
        assert_eq!(three.len(), 3);
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            before + 3,
            "入题库"
        );
        // 同 qid 幂等覆盖
        engine.add_chinglish_drills(3).await.expect("再注入");
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            before + 3,
            "重注入不重复"
        );
        // limit=0 全量
        let all = engine.add_chinglish_drills(0).await.expect("全量");
        assert!(all.len() >= 10, "内置错式 ≥10: {}", all.len());
        assert_eq!(
            engine.start_practice(0).await.expect("取题").len(),
            before + all.len(),
            "全量入库后计数"
        );
        // 训练题可作答：答案恒为地道改写
        let drill = all[0].clone();
        let QuestionKind::SingleChoice(body) = drill.kind.clone() else {
            panic!("单选改错")
        };
        engine.add_question(&drill).await.expect("注入");
        let out = engine
            .submit_answer(&attempt_for(drill.qid, body.answer, "drill-1"))
            .await
            .expect("提交");
        assert_eq!(out.grade.verdict, Verdict::Correct, "改错题可判");
    }
}
