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
    /// 题目判分口径 id（Essay=题面 rubric_id；Translation=内置 translation4；其它题型无 composite 口径 → None）
    pub fn rubric_id_for(kind: &QuestionKind) -> Option<String> {
        match kind {
            QuestionKind::Essay(body) => Some(body.rubric_id.clone()),
            QuestionKind::Translation(_) => Some(ambition_grader::TRANSLATION_RUBRIC.to_string()),
            _ => None,
        }
    }

    /// 建卷（M5-W8 docs/10 §5：字数红线 + 计时 + 收卷铃；纯件委托）
    pub fn exam_sheet(
        &self,
        prompt: ambition_core::EssayPrompt,
        body: String,
        started_at: i64,
    ) -> ambition_writing::ExamSheet {
        ambition_writing::exam_sheet(prompt, body, started_at)
    }

    /// 卷面违规扫描（Under/Over 字数红线 · Overtime 超时 · Pasted 禁粘贴）
    pub fn sheet_violations(
        &self,
        sheet: &ambition_writing::ExamSheet,
        now: i64,
    ) -> Vec<ambition_writing::Violation> {
        ambition_writing::sheet_violations(sheet, now)
    }

    /// 手动收卷（M5-W8 docs/10 §5：violations 非空 = 拒收不评分；干净卷面走 essay composite（W3 路径））
    pub async fn hand_in(
        &self,
        sheet: &ambition_writing::ExamSheet,
        now: i64,
    ) -> Result<GradeResult, Vec<ambition_writing::Violation>> {
        let violations = ambition_writing::sheet_violations(sheet, now);
        if !violations.is_empty() {
            return Err(violations);
        }
        let question = Question::new(
            "Write an essay in English according to the prompt.".into(),
            QuestionKind::Essay(sheet.prompt.clone()),
            3,
            vec!["essay".to_string(), sheet.prompt.genre.clone()],
            ambition_core::Provenance::Generated,
        );
        Ok(self
            .grader
            .grade(&question, &Answer::Text(sheet.body.clone()))
            .await)
    }

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
        // 事件（M5-W5 → gamify Xp(5)、不进连击，docs/13）。
        self.settle(DomainEvent::EssayGraded {
            essay_id: question.qid.0,
            rubric_id: sub.rubric_id.clone(),
            score: grade.score,
            max: grade.max,
        })
        .await;
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

    fn sheet_prompt() -> ambition_core::EssayPrompt {
        ambition_core::EssayPrompt {
            prompt_zh: "以绿色校园为题。".to_string(),
            genre: "argument".to_string(),
            track: "cet4".to_string(),
            rubric_id: "gaokao25".to_string(),
            word_range: (80, 120),
            minutes: 30,
        }
    }

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

    #[test]
    fn test_rubric_id_for_maps_composite_kinds_only() {
        let essay = QuestionKind::Essay(EssayPrompt {
            prompt_zh: "题面".into(),
            genre: "argument".into(),
            track: "cet4".into(),
            rubric_id: "gaokao25".into(),
            word_range: (120, 180),
            minutes: 30,
        });
        assert_eq!(
            Engine::rubric_id_for(&essay).as_deref(),
            Some("gaokao25"),
            "作文取题面口径"
        );
        let translation = QuestionKind::Translation(ambition_core::TranslationBody {
            direction: ambition_core::TranslationDirection {
                from: ambition_core::LangTag("zh-Hans".into()),
                to: ambition_core::LangTag("en".into()),
            },
            source_text: "s".into(),
            reference: "r".into(),
        });
        assert_eq!(
            Engine::rubric_id_for(&translation).as_deref(),
            Some(ambition_grader::TRANSLATION_RUBRIC),
            "翻译取内置口径"
        );
        let choice = QuestionKind::SingleChoice(ambition_core::ChoiceBody {
            options: vec![],
            answer: vec![],
        });
        assert_eq!(
            Engine::rubric_id_for(&choice),
            None,
            "选择题无 composite 口径"
        );
    }

    #[tokio::test]
    async fn test_exam_sheet_delegates_deadline_from_prompt() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let sheet = engine.exam_sheet(sheet_prompt(), "body words".to_string(), 1_000);
        assert_eq!(sheet.deadline, 1_000 + 30 * 60, "收卷铃 = 起始 + 分钟");
    }

    #[tokio::test]
    async fn test_sheet_violations_delegates_under_flag() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let sheet = engine.exam_sheet(sheet_prompt(), "two words".to_string(), 1_000);
        let violations = engine.sheet_violations(&sheet, 1_500);
        assert!(
            violations.contains(&ambition_writing::Violation::Under),
            "欠字数进违规: {violations:?}"
        );
    }

    #[tokio::test]
    async fn test_hand_in_rejects_violations_without_grading() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let sheet = engine.exam_sheet(sheet_prompt(), "only two".to_string(), 1_000);
        let err = engine.hand_in(&sheet, 1_500).await.expect_err("欠字数拒收");
        assert!(err.contains(&ambition_writing::Violation::Under), "{err:?}");
        let mut pasted = engine.exam_sheet(sheet_prompt(), "fine body ".repeat(100), 1_000);
        pasted.pasted = true;
        let err = engine.hand_in(&pasted, 1_500).await.expect_err("粘贴拒收");
        assert!(
            err.contains(&ambition_writing::Violation::Pasted),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn test_hand_in_clean_sheet_grades_via_composite_offline() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let body = "word ".repeat(100);
        let sheet = engine.exam_sheet(sheet_prompt(), body, 1_000);
        let grade = engine.hand_in(&sheet, 1_500).await.expect("干净卷面入判");
        assert_eq!(
            grade.verdict,
            ambition_core::Verdict::Pending,
            "离线 composite → Pending（不伪造）"
        );
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
