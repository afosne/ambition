//! Ambition gen：出题生成 + Jev 质检回路 + qpack（docs/06-generation，M2 落地范围）
//! 主流程：prompt → llm.structured（JSON Schema 约束草稿）→ 校验（qid=内容哈希）
//! → qc_question（Jev fan-out 单请求）→ 不合格重生成 ≤2 轮 → 交 engine 入库。

mod qpack;

pub use qpack::{
    IMPORT_QUALITY_FLOOR, ImportPlan, Manifest, PackItem, QPACK_SCHEMA_VERSION, QPack, build_qpack,
    parse_qpack, plan_import,
};

use ambition_jev::{JevClient, JevError, JevQuestion, JevQuestions, NoulQ, ScoreQ};
use ambition_llm::{GenRequest, JsonSchema, LlmError, LlmKind, LlmRouter};

/// 质检 noul 下限（docs/06 §2：noul ≥ 0.7）
pub const QC_NOUL_MIN: f64 = 0.7;
/// 质检 natural 分下限（docs/06 §2：natural ≥ 1.5；score levels 0..2）
pub const QC_NATURAL_MIN: f64 = 1.5;
/// 重生成轮数上限（docs/06 §1：质检回路 ≤2 轮）
pub const MAX_REGEN_ROUNDS: u32 = 2;

/// 考试轨（stages/<lang>/<track>，docs/16）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrackId(pub String);

/// 生成题型标签（prompt 与 schema 用的稳定字符串）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKindTag {
    SingleChoice,
    MultiChoice,
    FillBlank,
}

impl QuestionKindTag {
    /// 稳定字符串（prompt 拼接与草稿校验共用）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SingleChoice => "single_choice",
            Self::MultiChoice => "multi_choice",
            Self::FillBlank => "fill_blank",
        }
    }

    /// 是否选择类题型（决定质检是否问 distractor/unique）
    pub fn is_choice(&self) -> bool {
        matches!(self, Self::SingleChoice | Self::MultiChoice)
    }
}

/// 词集（source_words 输入）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WordSet(pub Vec<String>);

/// 生成规格（docs/06 §1；difficulty 必来自 CAT——M2 由调用方给定区间）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenSpec {
    pub track: TrackId,
    pub kinds: Vec<QuestionKindTag>,
    pub count: u32,
    pub difficulty: (u8, u8),
    pub topic: Option<String>,
    pub source_words: Option<WordSet>,
    pub weak_points: Vec<ambition_core::ErrorTag>,
    pub seed_qid: Option<ambition_core::Qid>,
}

impl GenSpec {
    /// 构造基础规格（difficulty/topic 等由调用方补字段）
    pub fn new(track: &str, kinds: Vec<QuestionKindTag>, count: u32) -> Self {
        Self {
            track: TrackId(track.to_string()),
            kinds,
            count,
            difficulty: (1, 3),
            topic: None,
            source_words: None,
            weak_points: Vec::new(),
            seed_qid: None,
        }
    }
}

/// LLM 草稿题目（结构化输出的中间形态；校验后转 Question）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DraftQuestion {
    pub kind: QuestionKindTag,
    pub stem: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub answers: Vec<String>,
    #[serde(default)]
    pub blanks: Vec<Vec<String>>,
    pub difficulty: u8,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// 结构化批量输出（顶层包装）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DraftBatch {
    pub questions: Vec<DraftQuestion>,
}

impl DraftQuestion {
    /// 校验并转 Question（qid = 内容哈希；任何结构问题 → Err 触发重生成）
    pub fn to_question(&self) -> Result<ambition_core::Question, GenError> {
        if self.stem.trim().is_empty() {
            return Err(GenError::InvalidDraft("empty stem".into()));
        }
        if !(1..=5).contains(&self.difficulty) {
            return Err(GenError::InvalidDraft(format!(
                "difficulty out of 1..=5: {}",
                self.difficulty
            )));
        }
        let kind = match self.kind {
            QuestionKindTag::SingleChoice | QuestionKindTag::MultiChoice => {
                let body = choice_body(self)?;
                if self.kind == QuestionKindTag::SingleChoice && body.answer.len() != 1 {
                    return Err(GenError::InvalidDraft(format!(
                        "single_choice needs exactly 1 answer, got {}",
                        body.answer.len()
                    )));
                }
                if self.kind == QuestionKindTag::MultiChoice && body.answer.len() < 2 {
                    return Err(GenError::InvalidDraft(
                        "multi_choice needs >=2 answers".into(),
                    ));
                }
                if self.kind == QuestionKindTag::SingleChoice {
                    ambition_core::QuestionKind::SingleChoice(body)
                } else {
                    ambition_core::QuestionKind::MultiChoice(body)
                }
            }
            QuestionKindTag::FillBlank => {
                if self.blanks.is_empty() {
                    return Err(GenError::InvalidDraft("fill_blank needs blanks".into()));
                }
                let markers = self.stem.matches("___").count();
                if markers != self.blanks.len() {
                    return Err(GenError::InvalidDraft(format!(
                        "stem has {markers} blanks but {} answer sets",
                        self.blanks.len()
                    )));
                }
                if self.blanks.iter().any(|set| set.is_empty()) {
                    return Err(GenError::InvalidDraft("blank without acceptable".into()));
                }
                ambition_core::QuestionKind::FillBlank(ambition_core::FillBody {
                    sentence: self.stem.clone(),
                    blanks: self
                        .blanks
                        .iter()
                        .map(|set| ambition_core::BlankItem {
                            acceptable: set.clone(),
                        })
                        .collect(),
                })
            }
        };
        Ok(ambition_core::Question::new(
            self.stem.clone(),
            kind,
            self.difficulty,
            self.tags.clone(),
            ambition_core::Provenance::Generated,
        ))
    }
}

/// 选项草稿 → ChoiceBody（答案以选项文本给出，按文本映射回 id；映射失败 = 草稿非法）
fn choice_body(draft: &DraftQuestion) -> Result<ambition_core::ChoiceBody, GenError> {
    if draft.options.len() < 2 {
        return Err(GenError::InvalidDraft("needs >=2 options".into()));
    }
    let mut texts = std::collections::HashSet::new();
    if !draft
        .options
        .iter()
        .all(|o| texts.insert(o.trim().to_string()))
    {
        return Err(GenError::InvalidDraft("duplicate option texts".into()));
    }
    if draft.answers.is_empty() {
        return Err(GenError::InvalidDraft("no answers".into()));
    }
    let mut answer_ids = Vec::with_capacity(draft.answers.len());
    for ans in &draft.answers {
        let id = draft
            .options
            .iter()
            .position(|o| o.trim() == ans.trim())
            .ok_or_else(|| GenError::InvalidDraft(format!("answer not in options: {ans}")))?
            as u32;
        if answer_ids.contains(&id) {
            return Err(GenError::InvalidDraft(format!("duplicate answer: {ans}")));
        }
        answer_ids.push(id);
    }
    Ok(ambition_core::ChoiceBody {
        options: draft
            .options
            .iter()
            .enumerate()
            .map(|(i, text)| ambition_core::OptionItem {
                id: i as u32,
                text: text.clone(),
            })
            .collect(),
        answer: answer_ids,
    })
}

/// 质检报告（docs/06 §2；None = 该题型不适用该检查）
#[derive(Debug, Clone, PartialEq)]
pub struct QcReport {
    pub unique_answer: Option<f64>,
    pub key_correct: f64,
    pub natural: f64,
    pub distractor_valid: Option<f64>,
    pub difficulty_fit: f64,
    pub passed: bool,
    pub quality_score: f32,
}

impl QcReport {
    /// 从 Jev 回答装配报告并判定（缺必答项 → Err；pass = noul≥0.7 且 natural≥1.5）
    pub fn from_answers(
        kind: &ambition_core::QuestionKind,
        answers: &ambition_jev::JevAnswers,
    ) -> Result<Self, JevError> {
        let is_choice = matches!(
            kind,
            ambition_core::QuestionKind::SingleChoice(_)
                | ambition_core::QuestionKind::MultiChoice(_)
        );
        for id in ["key_correct", "natural", "difficulty_fit"] {
            if !answers.contains_key(id) {
                return Err(JevError::Invalid(format!("missing qc answer: {id}")));
            }
        }
        let key_correct = answers
            .get("key_correct")
            .and_then(ambition_jev::JevAnswer::noul_prob)
            .ok_or_else(|| JevError::Invalid("key_correct not noul".into()))?;
        let natural = answers
            .get("natural")
            .and_then(ambition_jev::JevAnswer::score_value)
            .ok_or_else(|| JevError::Invalid("natural not score".into()))?;
        let difficulty_fit = answers
            .get("difficulty_fit")
            .and_then(ambition_jev::JevAnswer::score_value)
            .ok_or_else(|| JevError::Invalid("difficulty_fit not score".into()))?;

        let mut noul_values = vec![key_correct];
        let mut unique_answer = None;
        let mut distractor_valid = None;
        if is_choice {
            unique_answer = Some(
                answers
                    .get("unique_answer")
                    .and_then(ambition_jev::JevAnswer::noul_prob)
                    .ok_or_else(|| JevError::Invalid("missing unique_answer".into()))?,
            );
            distractor_valid = Some(
                answers
                    .get("distractor_valid")
                    .and_then(ambition_jev::JevAnswer::noul_prob)
                    .ok_or_else(|| JevError::Invalid("missing distractor_valid".into()))?,
            );
            noul_values.push(unique_answer.unwrap_or(0.0));
            noul_values.push(distractor_valid.unwrap_or(0.0));
        }

        let noul_mean = noul_values.iter().sum::<f64>() / noul_values.len() as f64;
        let natural_norm = (natural / 2.0).clamp(0.0, 1.0);
        let passed = noul_values.iter().all(|v| *v >= QC_NOUL_MIN) && natural >= QC_NATURAL_MIN;
        Ok(Self {
            unique_answer,
            key_correct,
            natural,
            distractor_valid,
            difficulty_fit,
            passed,
            quality_score: (((noul_mean + natural_norm) / 2.0) as f32).clamp(0.0, 1.0),
        })
    }
}

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

/// 质检 state（只放 l2：题干/选项/答案键；母语/提示永不进 state，docs/05 §3.5）
fn qc_state(question: &ambition_core::Question, kind: QuestionKindTag) -> ambition_jev::JevState {
    let value = match (&question.kind, kind.is_choice()) {
        (
            ambition_core::QuestionKind::SingleChoice(body)
            | ambition_core::QuestionKind::MultiChoice(body),
            true,
        ) => serde_json::json!({
            "stem": question.stem,
            "options": body.options.iter().map(|o| &o.text).collect::<Vec<_>>(),
            "answer_option_texts": body
                .answer
                .iter()
                .map(|id| body.options.iter().find(|o| o.id == *id).map(|o| o.text.clone()).unwrap_or_default())
                .collect::<Vec<_>>(),
            "difficulty": question.difficulty,
        }),
        (ambition_core::QuestionKind::FillBlank(fill), false) => serde_json::json!({
            "stem": fill.sentence,
            "acceptable_per_blank": fill
                .blanks
                .iter()
                .map(|b| &b.acceptable)
                .collect::<Vec<_>>(),
            "difficulty": question.difficulty,
        }),
        _ => serde_json::json!({"stem": question.stem}),
    };
    ambition_jev::JevState(value)
}

/// 质检问题集（选择类：unique + distractor；全类：key/natural/difficulty_fit）
fn qc_questions(kind: QuestionKindTag) -> JevQuestions {
    let mut qs = JevQuestions::new();
    qs.insert(
        "key_correct".into(),
        JevQuestion::Noul(NoulQ {
            instructions: "Is the provided answer key actually correct for this question?".into(),
        }),
    );
    qs.insert(
        "natural".into(),
        JevQuestion::Score(ScoreQ {
            instructions: "How natural and unambiguous is the sentence/stem?".into(),
            levels: vec![
                "broken, wrong, or ambiguous".into(),
                "acceptable but stiff".into(),
                "natural and idiomatic".into(),
            ],
        }),
    );
    qs.insert(
        "difficulty_fit".into(),
        JevQuestion::Score(ScoreQ {
            instructions: "Is the stated difficulty appropriate for a learner at this level?"
                .into(),
            levels: vec!["too easy".into(), "about right".into(), "too hard".into()],
        }),
    );
    if kind.is_choice() {
        qs.insert(
            "unique_answer".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "Is the answer set complete and exclusive — no other option or \
                     combination of options is also correct?"
                    .into(),
            }),
        );
        qs.insert(
            "distractor_valid".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "Are the non-answer options plausible yet clearly wrong?".into(),
            }),
        );
    }
    qs
}

/// prompt 模板渲染（data/prompts 可覆盖，内嵌兜底；{{l2}} 等占位 docs/05 §1）
fn render_prompt(spec: &GenSpec, needed: u32, failures: &[String]) -> String {
    let template = load_prompt();
    let kinds = spec
        .kinds
        .iter()
        .map(QuestionKindTag::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let topic_line = match &spec.topic {
        Some(topic) => format!("- Topic focus: {topic}.\n"),
        None => String::new(),
    };
    let feedback_line = if failures.is_empty() {
        String::new()
    } else {
        format!(
            "\nPrevious attempts were rejected:\n{}\nFix these issues.",
            failures
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let mut out = template
        .replace("{{count}}", &needed.to_string())
        .replace("{{kinds}}", &kinds)
        .replace("{{track}}", &spec.track.0)
        .replace(
            "{{difficulty}}",
            &format!("{}-{}", spec.difficulty.0, spec.difficulty.1),
        )
        .replace("{{l2}}", "English")
        .replace("{{topic_line}}", &topic_line)
        .replace("{{feedback_line}}", &feedback_line);
    if let Some(words) = &spec.source_words {
        out.push_str(&format!(
            "\nUse these target words where natural: {}.\n",
            words.0.join(", ")
        ));
    }
    if let Some(seed) = &spec.seed_qid {
        out.push_str(&format!(
            "\nCreate a new question on the same knowledge point as seed {}.\n",
            seed.0
        ));
    }
    if !spec.weak_points.is_empty() {
        let tags = spec
            .weak_points
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("\nTarget learner weak points: {tags}.\n"));
    }
    out
}

/// prompt 模板：优先 data/prompts/question_gen.md（cwd 相对），否则内嵌同名文件
fn load_prompt() -> String {
    std::fs::read_to_string("data/prompts/question_gen.md")
        .unwrap_or_else(|_| include_str!("../../../data/prompts/question_gen.md").to_string())
}

/// 草稿 JSON Schema（llm response_format 约束）
fn draft_schema() -> JsonSchema {
    serde_json::json!({
        "type": "object",
        "required": ["questions"],
        "properties": {
            "questions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["kind", "stem", "answers", "difficulty", "tags"],
                    "properties": {
                        "kind": {"type": "string", "enum": ["single_choice", "multi_choice", "fill_blank"]},
                        "stem": {"type": "string"},
                        "options": {"type": "array", "items": {"type": "string"}},
                        "answers": {"type": "array", "items": {"type": "string"}},
                        "blanks": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}},
                        "difficulty": {"type": "integer", "minimum": 1, "maximum": 5},
                        "tags": {"type": "array", "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}

/// 生成/质检错误（每 crate 一个 XxxError，docs/01 §1）
#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("llm: {0}")]
    Llm(#[from] LlmError),
    #[error("jev: {0}")]
    Jev(#[from] JevError),
    #[error("invalid draft: {0}")]
    InvalidDraft(String),
    #[error("invalid spec: {0}")]
    InvalidSpec(String),
    #[error("qpack: {0}")]
    Pack(String),
    #[error("all questions rejected after {rounds} regeneration round(s)")]
    AllRejected { rounds: u32 },
}

/// 反序列化结构化输出的辅助（provider 直出 JSON 字符串场景）
pub fn parse_draft_batch(text: &str) -> Result<DraftBatch, GenError> {
    serde_json::from_str(text).map_err(|e| GenError::InvalidDraft(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{BlankItem, FillBody, OptionItem, Provenance, QuestionKind};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn spec() -> GenSpec {
        GenSpec::new(
            "cet4",
            vec![QuestionKindTag::SingleChoice, QuestionKindTag::FillBlank],
            3,
        )
    }

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

    #[test]
    fn test_new_gen_spec_defaults() {
        let s = spec();
        assert_eq!(s.track, TrackId("cet4".into()));
        assert_eq!(s.kinds.len(), 2);
        assert_eq!(s.count, 3);
        assert_eq!(s.difficulty, (1, 3));
        assert!(s.topic.is_none());
        assert!(s.weak_points.is_empty());
        assert!(s.seed_qid.is_none());
    }

    #[test]
    fn test_as_str_and_is_choice_for_kind_tags() {
        assert_eq!(QuestionKindTag::SingleChoice.as_str(), "single_choice");
        assert_eq!(QuestionKindTag::FillBlank.as_str(), "fill_blank");
        assert!(QuestionKindTag::SingleChoice.is_choice());
        assert!(!QuestionKindTag::FillBlank.is_choice());
        let json = serde_json::to_value(QuestionKindTag::MultiChoice).expect("序列化");
        assert_eq!(json, serde_json::json!("multi_choice"), "snake_case");
    }

    #[test]
    fn test_to_question_builds_single_choice_with_content_qid() {
        let draft: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "single_choice",
            "stem": "She ___ daily.",
            "options": ["run", "runs"],
            "answers": ["runs"],
            "difficulty": 2,
            "tags": ["grammar"]
        }))
        .expect("草稿");
        let q = draft.to_question().expect("转换");
        match &q.kind {
            QuestionKind::SingleChoice(body) => {
                assert_eq!(body.answer, vec![1]);
                assert_eq!(body.options.len(), 2);
            }
            other => panic!("题型错误: {other:?}"),
        }
        assert_eq!(q.provenance, Provenance::Generated);
        assert_eq!(q.quality_score, 1.0, "qc 前默认满分");
        // 同内容同 qid
        let again = draft.to_question().expect("转换2");
        assert_eq!(again.qid, q.qid);
    }

    #[test]
    fn test_to_question_rejects_malformed_drafts() {
        let base = serde_json::json!({
            "kind": "single_choice",
            "stem": "S.",
            "options": ["a", "b"],
            "answers": ["a"],
            "difficulty": 2,
            "tags": []
        });
        let mut bad = base.clone();
        bad["answers"] = serde_json::json!(["not-an-option"]);
        assert!(
            serde_json::from_value::<DraftQuestion>(bad)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "答案不在选项中 → Err"
        );

        let mut empty_stem = base.clone();
        empty_stem["stem"] = serde_json::json!("   ");
        assert!(
            serde_json::from_value::<DraftQuestion>(empty_stem)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "空题干 → Err"
        );

        let mut dup = base.clone();
        dup["options"] = serde_json::json!(["a", "a"]);
        assert!(
            serde_json::from_value::<DraftQuestion>(dup)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "重复选项 → Err"
        );

        let single_one = serde_json::json!({
            "kind": "single_choice", "stem": "S.", "options": ["a", "b", "c"],
            "answers": ["a", "b"], "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(single_one)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "单选多答 → Err"
        );

        let multi_one = serde_json::json!({
            "kind": "multi_choice", "stem": "S.", "options": ["a", "b"],
            "answers": ["a"], "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(multi_one)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "多选单答 → Err"
        );

        let fill_marker_mismatch = serde_json::json!({
            "kind": "fill_blank", "stem": "She ___ every ___ day.",
            "answers": [], "blanks": [["runs"]], "difficulty": 2, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(fill_marker_mismatch)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "空位数不匹配 → Err"
        );

        let bad_difficulty = serde_json::json!({
            "kind": "single_choice", "stem": "S.", "options": ["a", "b"],
            "answers": ["a"], "difficulty": 9, "tags": []
        });
        assert!(
            serde_json::from_value::<DraftQuestion>(bad_difficulty)
                .expect("可反序列化")
                .to_question()
                .is_err(),
            "难度越界 → Err"
        );
    }

    #[test]
    fn test_to_question_builds_fill_blank() {
        let draft: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "fill_blank",
            "stem": "She ___ to school. He ___ home.",
            "answers": [],
            "blanks": [["goes", "walks"], ["stays"]],
            "difficulty": 3,
            "tags": ["grammar"]
        }))
        .expect("草稿");
        let q = draft.to_question().expect("转换");
        match &q.kind {
            QuestionKind::FillBlank(fill) => {
                assert_eq!(fill.blanks.len(), 2);
                assert_eq!(fill.blanks[0].acceptable, vec!["goes", "walks"]);
            }
            other => panic!("题型错误: {other:?}"),
        }
        assert_eq!(q.stem, "She ___ to school. He ___ home.");
    }

    #[test]
    fn test_from_answers_pass_and_fail_thresholds() {
        let kind = QuestionKind::SingleChoice(ambition_core::ChoiceBody {
            options: vec![
                OptionItem {
                    id: 0,
                    text: "a".into(),
                },
                OptionItem {
                    id: 1,
                    text: "b".into(),
                },
            ],
            answer: vec![0],
        });
        let pass_answers = passing_choice_answers();
        let report = QcReport::from_answers(&kind, &pass_answers).expect("装配");
        assert!(report.passed, "全过阈值: {report:?}");
        assert!(
            report.quality_score > 0.8,
            "高质量分: {}",
            report.quality_score
        );

        let mut fail = pass_answers.clone();
        fail.insert(
            "unique_answer".into(),
            ambition_jev::JevAnswer::Noul { yes_prob: 0.3 },
        );
        let report2 = QcReport::from_answers(&kind, &fail).expect("装配");
        assert!(!report2.passed, "unique_answer < 0.7 应拒");

        let mut fail2 = passing_choice_answers();
        fail2.insert(
            "natural".into(),
            ambition_jev::JevAnswer::Score {
                score: 1.0,
                confidence: 0.9,
            },
        );
        let report3 = QcReport::from_answers(&kind, &fail2).expect("装配");
        assert!(!report3.passed, "natural < 1.5 应拒");
    }

    fn passing_choice_answers() -> ambition_jev::JevAnswers {
        let mut answers = ambition_jev::JevAnswers::new();
        answers.insert(
            "key_correct".into(),
            ambition_jev::JevAnswer::Noul { yes_prob: 0.95 },
        );
        answers.insert(
            "natural".into(),
            ambition_jev::JevAnswer::Score {
                score: 1.9,
                confidence: 0.9,
            },
        );
        answers.insert(
            "difficulty_fit".into(),
            ambition_jev::JevAnswer::Score {
                score: 2.0,
                confidence: 0.8,
            },
        );
        answers.insert(
            "unique_answer".into(),
            ambition_jev::JevAnswer::Noul { yes_prob: 0.92 },
        );
        answers.insert(
            "distractor_valid".into(),
            ambition_jev::JevAnswer::Noul { yes_prob: 0.85 },
        );
        answers
    }

    #[test]
    fn test_from_answers_fill_blank_skips_choice_checks() {
        let fill = QuestionKind::FillBlank(FillBody {
            sentence: "She ___ .".into(),
            blanks: vec![BlankItem {
                acceptable: vec!["runs".into()],
            }],
        });
        let mut answers = ambition_jev::JevAnswers::new();
        answers.insert(
            "key_correct".into(),
            ambition_jev::JevAnswer::Noul { yes_prob: 0.9 },
        );
        answers.insert(
            "natural".into(),
            ambition_jev::JevAnswer::Score {
                score: 1.8,
                confidence: 0.9,
            },
        );
        answers.insert(
            "difficulty_fit".into(),
            ambition_jev::JevAnswer::Score {
                score: 1.5,
                confidence: 0.7,
            },
        );
        let report = QcReport::from_answers(&fill, &answers).expect("装配");
        assert!(report.passed);
        assert!(report.unique_answer.is_none());
        assert!(report.distractor_valid.is_none());

        let missing = ambition_jev::JevAnswers::new();
        assert!(
            QcReport::from_answers(&fill, &missing).is_err(),
            "缺必答项 → Err（P-8：不静默降级）"
        );
    }

    #[test]
    fn test_render_prompt_substitutes_placeholders_and_feedback() {
        let mut s = spec();
        s.topic = Some("campus life".into());
        s.source_words = Some(WordSet(vec!["dormitory".into(), "tuition".into()]));
        s.weak_points = vec![ambition_core::ErrorTag::Tense];
        s.seed_qid = Some(ambition_core::Qid("deadbeef".into()));
        let prompt = render_prompt(&s, 2, &["qc failed: stem ambiguous".into()]);
        assert!(!prompt.contains("{{count}}"), "占位全部替换: {prompt}");
        assert!(prompt.contains("exactly 2 single_choice, fill_blank"));
        assert!(prompt.contains("cet4"));
        assert!(prompt.contains("1-3"));
        assert!(prompt.contains("campus life"));
        assert!(prompt.contains("Previous attempts were rejected"));
        assert!(prompt.contains("dormitory"));
        assert!(prompt.contains("deadbeef"));
        assert!(prompt.contains("tense"));
        assert!(prompt.contains("English"));

        let clean = render_prompt(
            &GenSpec::new("gaokao", vec![QuestionKindTag::FillBlank], 1),
            1,
            &[],
        );
        assert!(!clean.contains("Previous attempts"), "无失败则无反馈段");
        assert!(!clean.contains("{{"));
    }

    #[test]
    fn test_load_prompt_falls_back_to_embedded_template() {
        let text = load_prompt();
        assert!(text.contains("{{count}}"), "模板占位在场");
        assert!(text.contains("English"), "英文模板");
    }

    #[test]
    fn test_draft_schema_requires_core_fields() {
        let schema = draft_schema();
        let required = schema["properties"]["questions"]["items"]["required"]
            .as_array()
            .expect("required 数组");
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"kind"));
        assert!(names.contains(&"stem"));
        assert!(names.contains(&"answers"));
        assert_eq!(
            schema["properties"]["questions"]["items"]["properties"]["kind"]["enum"][0],
            "single_choice"
        );
    }

    #[test]
    fn test_to_question_maps_choice_answers_by_text() {
        let draft = DraftQuestion {
            kind: QuestionKindTag::MultiChoice,
            stem: "Pick two.".into(),
            options: vec!["x".into(), "y".into(), "z".into()],
            answers: vec!["z".into(), "x".into()],
            blanks: vec![],
            difficulty: 2,
            tags: vec![],
        };
        let body = choice_body(&draft).expect("映射");
        assert_eq!(body.answer, vec![2, 0], "按文本映射回 id");
        assert_eq!(body.options[0].id, 0);
    }

    #[test]
    fn test_qc_state_contains_only_l2_fields() {
        let draft = DraftQuestion {
            kind: QuestionKindTag::SingleChoice,
            stem: "Pick one.".into(),
            options: vec!["alpha".into(), "beta".into()],
            answers: vec!["alpha".into()],
            blanks: vec![],
            difficulty: 2,
            tags: vec![],
        };
        let q = draft.to_question().expect("题");
        let state = qc_state(&q, QuestionKindTag::SingleChoice);
        let text = state.0.to_string();
        assert!(text.contains("Pick one."));
        assert!(text.contains("alpha"));
        assert!(
            !text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "state 无 CJK: {text}"
        );
        assert!(qc_questions(QuestionKindTag::SingleChoice).contains_key("unique_answer"));
        assert!(!qc_questions(QuestionKindTag::FillBlank).contains_key("unique_answer"));
    }

    #[test]
    fn test_parse_draft_batch_roundtrip() {
        let batch = parse_draft_batch(
            r#"{"questions":[{"kind":"fill_blank","stem":"I ___ .","answers":[],
               "blanks":[["go"]],"difficulty":1,"tags":[]}]}"#,
        )
        .expect("解析");
        assert_eq!(batch.questions.len(), 1);
        assert!(parse_draft_batch("not json").is_err());
    }

    #[tokio::test]
    async fn test_qc_questions_differ_by_kind() {
        let choice = qc_questions(QuestionKindTag::SingleChoice);
        let fill = qc_questions(QuestionKindTag::FillBlank);
        assert!(choice.contains_key("unique_answer"));
        assert!(choice.contains_key("distractor_valid"));
        assert!(choice.contains_key("key_correct"));
        assert!(choice.contains_key("natural"));
        assert!(choice.contains_key("difficulty_fit"));
        assert_eq!(choice.len(), 5, "选择类五问 fan-out");
        assert!(!fill.contains_key("unique_answer"), "填空无干扰项检查");
        assert!(!fill.contains_key("distractor_valid"));
        assert_eq!(fill.len(), 3, "填空三问 fan-out");
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

    #[test]
    fn test_choice_body_maps_answers_by_text() {
        let draft = DraftQuestion {
            kind: QuestionKindTag::MultiChoice,
            stem: "Pick two.".into(),
            options: vec!["x".into(), "y".into(), "z".into()],
            answers: vec!["z".into(), "x".into()],
            blanks: vec![],
            difficulty: 2,
            tags: vec![],
        };
        let body = choice_body(&draft).expect("映射");
        assert_eq!(body.answer, vec![2, 0], "按文本映射回 id");
        assert_eq!(body.options[0].id, 0);
    }

    #[test]
    fn test_is_choice_drives_qc_question_set() {
        assert!(QuestionKindTag::SingleChoice.is_choice());
        assert!(QuestionKindTag::MultiChoice.is_choice());
        assert!(!QuestionKindTag::FillBlank.is_choice());
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
}
