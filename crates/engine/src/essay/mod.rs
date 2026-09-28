//! 作文批改闭环门面（M5-W5，docs/10 §2、docs/03 写作（S3））：
//! grade_essay（07 composite → 档位 + 落两表 + EssayGraded 事件）/ annotate（LLM 缓存走 run_loop）
//! / upgrade_sentence（本地反模板+同义表）/ verify_upgrade（Jev noul 原意）/ model_essays（三档范文 structured）。
//! 自由作文无轨：track/字数红线/限时归卷面层（W8）；离线一律离线矩阵不伪造（P-8）。

use ambition_core::{Answer, EssayPrompt, QuestionKind, now_unix};
use ambition_writing::{
    Comments, EssaySubmission, ModelEssaySet, Upgrade, annotate_prompt, builtin_templates,
    comments_schema, model_essay_prompt, model_essay_schema, upgrade_sentence,
};

use crate::*;

impl Engine {
    /// 自由作文批改（M5-W5：07 composite → 档位；落 essays+essay_grades 两表 + 发 DomainEvent::EssayGraded）
    pub async fn grade_essay(&self, sub: &EssaySubmission) -> Result<GradeResult, EngineError> {
        let word_count = sub.body.split_whitespace().count() as i64;
        // 合成题：qid 含正文与纳秒时钟（同 prompt 不同正文/重交不撞 PK；未入 questions 表——仅判分与落库用）
        let unique = format!(
            "essay-{}-{:x}",
            ambition_core::canonical_json_hash(&serde_json::json!({
                "prompt": sub.prompt_zh,
                "body": sub.body,
            }))
            .0,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        let question = Question {
            qid: ambition_core::Qid(unique),
            kind: QuestionKind::Essay(EssayPrompt {
                prompt_zh: sub.prompt_zh.clone(),
                genre: sub.genre.clone(),
                track: String::new(), // 自由作文无轨（轨/字数红线随卷面层，W8）
                rubric_id: sub.rubric_id.clone(),
                word_range: (0, u16::MAX),
                minutes: 0,
            }),
            stem: "Write an essay in English according to the prompt.".to_string(),
            difficulty: 3,
            tags: vec!["essay".to_string(), sub.genre.clone()],
            provenance: ambition_core::Provenance::Generated,
            quality_score: 1.0,
            lang: ambition_core::LangTag::english(),
        };
        let grade = self
            .grader
            .grade(&question, &Answer::Text(sub.body.clone()))
            .await;
        ambition_store::insert_essay(
            &self.pool,
            &ambition_store::NewEssay {
                essay_id: question.qid.0.clone(),
                qid: None,
                prompt: sub.prompt_zh.clone(),
                genre: sub.genre.clone(),
                body: sub.body.clone(),
                word_count,
                scaffold_stage: sub.scaffold_stage.clone(),
                template_id: sub.template_id.clone(),
                created_at: now_unix(),
            },
            Some(&grade),
        )
        .await?;
        // 事件（M5-W5 → gamify Xp(5)、不进连击，docs/13；中毒取内层可见，P-8）
        ambition_core::lock_recover(&self.events, "Engine.events").push(DomainEvent::EssayGraded {
            essay_id: question.qid.0,
            rubric_id: sub.rubric_id.clone(),
            score: grade.score,
            max: grade.max,
        });
        Ok(grade)
    }

    /// 逐句批注（LlmKind::Explain + writing 资产 schema；缓存 = llm run_loop effective_cache_key 自动口径）
    pub async fn annotate(
        &self,
        essay: &str,
        grade: &GradeResult,
        rubric_id: &str,
    ) -> Result<Comments, EngineError> {
        let Some(llm) = &self.llm else {
            return Err(EngineError::Offline);
        };
        let l2 = ambition_core::LangTag::english();
        let (instruction, prompt) = annotate_prompt(&l2, essay, rubric_id, grade);
        let mut req =
            ambition_llm::GenRequest::new(ambition_llm::LlmKind::Explain, instruction, prompt);
        req.schema = Some(comments_schema());
        let comments: Comments = llm.structured(req).await.map_err(from_llm)?;
        Ok(comments)
    }

    /// 升级句候选（本地反模板库 + 同义表，零网络；未命中 → 空列表不臆造）
    pub async fn upgrade_sentence(&self, sentence: &str) -> Result<Vec<Upgrade>, EngineError> {
        Ok(upgrade_sentence(sentence, builtin_templates()))
    }

    /// 升级句核验（Jev noul「原意未变」≥0.5 过；语义走样拦下——checklist）
    pub async fn verify_upgrade(&self, from: &str, to: &str) -> Result<bool, EngineError> {
        self.grader.verify_upgrade(from, to).await.map_err(from_jev)
    }

    /// 三档范文 + 逐句赏析（llm structured + writing schema；200/400/600）
    pub async fn model_essays(&self, prompt: &str) -> Result<ModelEssaySet, EngineError> {
        let Some(llm) = &self.llm else {
            return Err(EngineError::Offline);
        };
        let l2 = ambition_core::LangTag::english();
        let (instruction, user) = model_essay_prompt(&l2, prompt);
        let mut req =
            ambition_llm::GenRequest::new(ambition_llm::LlmKind::Explain, instruction, user);
        req.schema = Some(model_essay_schema());
        let set: ModelEssaySet = llm.structured(req).await.map_err(from_llm)?;
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_writing::EssaySubmission;

    fn submission(body: &str) -> EssaySubmission {
        EssaySubmission {
            body: body.to_string(),
            prompt_zh: "以“绿色校园”为题写一篇短文。".to_string(),
            genre: "argument".to_string(),
            rubric_id: "gaokao25".to_string(),
            scaffold_stage: "half".to_string(),
            template_id: "cet4_argument_procon".to_string(),
        }
    }

    #[tokio::test]
    async fn test_grade_essay_persists_rows_publishes_event_and_returns_grade() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let sub = submission(
            "Nowadays more students use mobile phones. In my opinion, we should use them wisely and keep a balance between study and relaxation, because only in this way can we focus on our lessons and stay healthy.",
        );
        let grade = engine.grade_essay(&sub).await.expect("离线也出档位");
        // 无 Jev → composite Pending（不伪造），但两表与事件照落
        assert_eq!(grade.verdict, Verdict::Pending, "离线 → Pending");
        assert!(
            matches!(grade.detail, ambition_core::GradeDetail::Items(v) if v.is_empty()),
            "离线桶口径（pending_grade 空明细；Dims 仅成功路径）"
        );

        let essays: i64 = sqlx::query_scalar("SELECT count(*) FROM essays")
            .fetch_one(&engine.pool)
            .await
            .expect("essays 计数");
        let grades: i64 = sqlx::query_scalar("SELECT count(*) FROM essay_grades")
            .fetch_one(&engine.pool)
            .await
            .expect("essay_grades 计数");
        assert_eq!((essays, grades), (1, 1), "两表同落");
        let (prompt, body): (String, String) =
            sqlx::query_as("SELECT prompt, body FROM essays LIMIT 1")
                .fetch_one(&engine.pool)
                .await
                .expect("行读取");
        assert_eq!(prompt, "以“绿色校园”为题写一篇短文。", "题面持久化");
        assert!(body.starts_with("Nowadays"), "正文持久化");

        let events = engine.drain_events();
        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, DomainEvent::EssayGraded { .. })),
            "EssayGraded 已发布（gamify Xp(5) 消费）: {events:?}"
        );
        // 历史回看
        let history =
            ambition_store::essay_history(&engine.pool, "以“绿色校园”为题写一篇短文。", 10)
                .await
                .expect("历史");
        assert_eq!(history.len(), 1, "同 prompt 历史 1 篇");
        assert!(history[0].grade.is_some(), "已批 grade 随史");
    }

    #[tokio::test]
    async fn test_annotate_offline_without_llm() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let grade = GradeResult {
            verdict: Verdict::Correct,
            score: 20.0,
            max: 25.0,
            confidence: 0.9,
            graded_by: ambition_core::GradedBy::Jev,
            detail: ambition_core::GradeDetail::Plain,
            error_tags: vec![],
        };
        let err = engine
            .annotate("Nowadays it is good.", &grade, "gaokao25")
            .await
            .expect_err("无 LLM → Offline");
        assert!(matches!(err, EngineError::Offline), "离线矩阵: {err}");
    }

    #[tokio::test]
    async fn test_upgrade_sentence_hits_builtin_antipattern_locally() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let ups = engine
            .upgrade_sentence(
                "Nowadays with the development of society and economy, we face many problems.",
            )
            .await
            .expect("本地候选");
        assert_eq!(ups.len(), 1, "内置反模板命中一条（零网络）");
        assert!(
            ups[0].to.starts_with("In recent years"),
            "替换句: {}",
            ups[0].to
        );

        let none = engine
            .upgrade_sentence("We protect the wetland basin carefully.")
            .await
            .expect("无命中");
        assert!(none.is_empty(), "无命中不臆造");
    }

    #[tokio::test]
    async fn test_verify_upgrade_offline_without_jev() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let err = engine
            .verify_upgrade("a", "b")
            .await
            .expect_err("无 Jev → Offline");
        assert!(matches!(err, EngineError::Offline), "verify: {err}");
    }

    #[tokio::test]
    async fn test_model_essays_offline_without_llm() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let err = engine
            .model_essays("Should phones be banned on campus?")
            .await
            .expect_err("无 LLM → Offline");
        assert!(matches!(err, EngineError::Offline), "model_essays: {err}");
    }
}
