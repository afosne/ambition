//! 三级判题流水线（docs/07-grading §1/§3/§4，M3 落地）：规则 → Jev → LLM 复核/待裁定桶。
//! `grade()` 永不 Err：AI 缺席/网络失败按离线矩阵降级（规则结果或 Pending 进桶，tracing 可见）。

use ambition_core::{
    Answer, ErrorTag, GradeResult, GradedBy, ItemResult, Question, QuestionKind, Verdict,
};
use ambition_jev::Route;
use ambition_jev::{
    JevClient, JevError, JevQuestion, JevQuestions, JevState, NoulQ, route_from_margin,
};
use ambition_llm::{GenRequest, LlmError, LlmKind, LlmRouter};

use crate::corpus::{error_cause_questions, tag_error_cause};
use crate::grade_rule;

/// LLM 复核的中性置信（无概率口径；低于 Jev 高置信路径，docs/07 §4）
pub const LLM_JUDGE_CONFIDENCE: f32 = 0.5;

/// 判分阈值（docs/05 §3：英语 0.75/0.35，其它语种更保守 0.85/0.20）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub accept: f32,
    pub reject: f32,
}

impl Thresholds {
    /// 英语默认阈值
    pub fn english() -> Self {
        Self {
            accept: 0.75,
            reject: 0.35,
        }
    }

    /// 非英语阈值（更多灰区 → 更多复核/待裁定）
    pub fn non_english() -> Self {
        Self {
            accept: 0.85,
            reject: 0.20,
        }
    }

    /// 按目标语选阈值（en → 英语档；其它 → 非英语档）
    pub fn for_lang(lang: &str) -> Self {
        if lang == "en" {
            Self::english()
        } else {
            Self::non_english()
        }
    }
}

/// 逐空判断（jev_blank 产出：可接受概率 + 该空错因）
#[derive(Debug, Clone, PartialEq)]
pub struct BlankJudge {
    pub blank_index: usize,
    pub yes_prob: f32,
    pub error_tags: Vec<ErrorTag>,
}

/// 漏选项判断（jev_multi_items：belongs = 模型倾向该选项属于正确答案集）
#[derive(Debug, Clone, PartialEq)]
pub struct ItemJudge {
    pub index: u32,
    pub belongs: bool,
    pub margin: f32,
}

/// 多选漏选判断集（逐项 + 错因，docs/07 §3）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MultiJudge {
    pub items: Vec<ItemJudge>,
    pub error_tags: Vec<ErrorTag>,
}

/// 第三级输入：暂定判定态（可回退的分与中间产物；灰区/离线时交 LLM 或进桶）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Provisional {
    pub score: f32,
    pub max: f32,
    pub items: Vec<ItemResult>,
    pub error_tags: Vec<ErrorTag>,
}

impl Provisional {
    /// 基本分构造（items/tags 默认空）
    pub fn new(score: f32, max: f32) -> Self {
        Self {
            score,
            max,
            items: Vec::new(),
            error_tags: Vec::new(),
        }
    }
}

/// 判题器（AI 全可空 = 离线形态；阈值按题目 lang 取）
pub struct Grader {
    jev: Option<std::sync::Arc<JevClient>>,
    llm: Option<std::sync::Arc<LlmRouter>>,
    thresholds: Thresholds,
}

impl Grader {
    /// 构造（jev/llm 可空；thresholds 按 lang 覆盖由调用方在 grade 时决定默认档）
    pub fn new(
        jev: Option<std::sync::Arc<JevClient>>,
        llm: Option<std::sync::Arc<LlmRouter>>,
        thresholds: Thresholds,
    ) -> Self {
        Self {
            jev,
            llm,
            thresholds,
        }
    }

    /// 离线形态（无 Jev 无 LLM：规则 + 待裁定桶）
    pub fn offline() -> Self {
        Self::new(None, None, Thresholds::english())
    }

    /// 生效阈值（按题目目标语；默认英语档）
    fn effective_thresholds(&self, question: &Question) -> Thresholds {
        if self.thresholds == Thresholds::english() {
            Thresholds::for_lang(&question.lang.0)
        } else {
            self.thresholds
        }
    }

    /// 灰区降级：无 LLM 时 LlmReview → HumanReview（进桶，docs/05 §2 语义）
    fn downgrade_route(&self, route: Route) -> Route {
        match route {
            Route::LlmReview if self.llm.is_none() => Route::HumanReview,
            other => other,
        }
    }

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
        let questions = error_cause_questions(vec![]);
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

    /// 填空 Jev 升级：逐空 acceptable 判（错空才问）→ 翻案/确认/灰区；确认错的空带错因
    async fn grade_fill_jev(
        &self,
        question: &Question,
        answer: &Answer,
        mut result: GradeResult,
    ) -> GradeResult {
        let thresholds = self.effective_thresholds(question);
        let mut min_margin = f32::MAX;
        let rule_score = result.score;
        let max = result.max;
        let mut flipped = 0usize;
        let mut undecided = false;
        for slot in 0..result.items.len() {
            if result.items[slot].correct {
                continue;
            }
            let blank_index = result.items[slot].index;
            let typed = match answer {
                Answer::Fill(values) => values.get(blank_index).cloned().unwrap_or_default(),
                _ => String::new(),
            };
            let judge = match self.jev_blank(question, blank_index, &typed).await {
                Ok(judge) => judge,
                Err(err) => {
                    // Jev 中途失败 = Jev 离线：保持规则结果（离线矩阵），标签缺失可见
                    tracing::warn!(error = %err, "jev_blank failed, keeping rule result");
                    return result;
                }
            };
            min_margin = min_margin.min(judge.yes_prob);
            match self.downgrade_route(route_from_margin(
                judge.yes_prob,
                thresholds.accept,
                thresholds.reject,
            )) {
                Route::AutoAccept => {
                    result.items[slot].correct = true;
                    flipped += 1;
                }
                Route::AutoReject => {
                    for tag in judge.error_tags {
                        if !result.error_tags.contains(&tag) {
                            result.error_tags.push(tag);
                        }
                    }
                }
                Route::LlmReview => undecided = true,
                Route::HumanReview => {
                    return pending_grade(rule_score, max, result.items, result.error_tags);
                }
            }
        }
        if undecided {
            return self
                .third_level(
                    question,
                    answer,
                    Provisional {
                        score: rule_score,
                        max,
                        items: result.items,
                        error_tags: result.error_tags,
                    },
                )
                .await;
        }
        let score = rule_score + flipped as f32;
        let verdict = if flipped == 0 {
            Verdict::Wrong // 全部确认错：维持规则结论
        } else if score >= max {
            Verdict::Acceptable // Jev 模糊通过
        } else {
            Verdict::Partial
        };
        GradeResult {
            verdict,
            score,
            max,
            confidence: if min_margin == f32::MAX {
                result.confidence
            } else {
                min_margin
            },
            graded_by: GradedBy::Jev,
            items: result.items,
            error_tags: result.error_tags,
        }
    }

    /// 单空可接受性 + 错因（一次请求；docs/07 §3 jev_blank；无 Jev → Offline）
    pub async fn jev_blank(
        &self,
        question: &Question,
        blank_index: usize,
        typed: &str,
    ) -> Result<BlankJudge, JevError> {
        let QuestionKind::FillBlank(fill) = &question.kind else {
            return Err(JevError::Invalid(
                "jev_blank requires a fill_blank question".into(),
            ));
        };
        let blank = fill
            .blanks
            .get(blank_index)
            .ok_or_else(|| JevError::Invalid(format!("blank {blank_index} out of range")))?;
        let Some(jev) = &self.jev else {
            return Err(JevError::Offline);
        };
        let state = JevState(serde_json::json!({
            "sentence": question.stem,
            "reference_answers": blank.acceptable,
            "student_answer": typed,
            "difficulty": question.difficulty,
        }));
        let mut questions = JevQuestions::new();
        questions.insert(
            "acceptable".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "The student's answer fits the sentence grammatically and preserves the intended meaning of the blank.".into(),
            }),
        );
        let questions = merge_error_cause(questions);
        let answers = jev.evaluate(state, &questions).await?;
        let yes_prob = answers
            .get("acceptable")
            .and_then(|a| a.noul_prob())
            .ok_or_else(|| JevError::Invalid("missing acceptable answer".into()))?
            as f32;
        Ok(BlankJudge {
            blank_index,
            yes_prob,
            error_tags: tag_error_cause(&answers, self.thresholds.accept),
        })
    }

    /// 多选漏选项逐项 noul + 错因（一次请求；docs/07 §3 jev_multi_items；无 Jev → Offline）
    pub async fn jev_multi_items(
        &self,
        question: &Question,
        selected: &[u32],
    ) -> Result<MultiJudge, JevError> {
        let QuestionKind::MultiChoice(body) = &question.kind else {
            return Err(JevError::Invalid(
                "jev_multi_items requires a multi_choice question".into(),
            ));
        };
        let missed: Vec<u32> = body
            .answer
            .iter()
            .filter(|id| !selected.contains(id))
            .copied()
            .collect();
        if missed.is_empty() {
            return Err(JevError::Invalid("no missed options to judge".into()));
        }
        let Some(jev) = &self.jev else {
            return Err(JevError::Offline);
        };
        let state = answer_state(question, &Answer::Choice(selected.to_vec()));
        let mut extra: Vec<(String, JevQuestion)> = Vec::with_capacity(missed.len());
        for id in &missed {
            extra.push((
                format!("belongs_{id}"),
                JevQuestion::Noul(NoulQ {
                    instructions:
                        "This option (see options in state) is part of the fully correct answer set; a thorough student should select it.".into(),
                }),
            ));
        }
        let questions = error_cause_questions(extra);
        let answers = jev.evaluate(state, &questions).await?;
        let mut items = Vec::with_capacity(missed.len());
        for id in missed {
            let yes_prob = answers
                .get(&format!("belongs_{id}"))
                .and_then(|a| a.noul_prob())
                .ok_or_else(|| JevError::Invalid(format!("missing belongs_{id}")))?;
            items.push(ItemJudge {
                index: id,
                belongs: yes_prob >= 0.5,
                margin: yes_prob as f32,
            });
        }
        Ok(MultiJudge {
            items,
            error_tags: tag_error_cause(&answers, self.thresholds.accept),
        })
    }

    /// 第三级：LLM 粗判（判断不打分）；不可用/失败 → 待裁定桶（docs/07 §4）
    async fn third_level(
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

/// 规则层不可判定时的 Pending（进 arbitration 桶；confidence=0 标低置信）
fn pending_grade(
    score: f32,
    max: f32,
    items: Vec<ItemResult>,
    error_tags: Vec<ErrorTag>,
) -> GradeResult {
    GradeResult {
        verdict: Verdict::Pending,
        score,
        max,
        confidence: 0.0,
        graded_by: GradedBy::Rule,
        items,
        error_tags,
    }
}

/// 多选判断集的最小 margin（整体置信 = 最弱判断）
fn min_margin(items: &[ItemJudge]) -> f32 {
    items.iter().map(|i| i.margin).fold(1.0, f32::min)
}

/// 拆作答（choice ids / fill 文本）
fn split_answer(answer: &Answer) -> (Vec<u32>, Vec<String>) {
    match answer {
        Answer::Choice(ids) => (ids.clone(), Vec::new()),
        Answer::Fill(values) => (Vec::new(), values.clone()),
    }
}

/// 追加 error_cause 判断到集合
fn merge_error_cause(mut questions: JevQuestions) -> JevQuestions {
    use crate::corpus::error_cause_question;
    questions.insert("error_cause".into(), error_cause_question());
    questions
}

/// Jev state：只放 l2 内容（题干/选项/参考答案/学生作答，docs/05 §3.5）
fn answer_state(question: &Question, answer: &Answer) -> JevState {
    let mut state = serde_json::Map::new();
    state.insert("stem".into(), serde_json::json!(question.stem));
    state.insert("difficulty".into(), serde_json::json!(question.difficulty));
    match &question.kind {
        QuestionKind::SingleChoice(body) | QuestionKind::MultiChoice(body) => {
            let options: Vec<serde_json::Value> = body
                .options
                .iter()
                .map(|o| serde_json::json!({"id": o.id, "text": o.text}))
                .collect();
            state.insert("options".into(), serde_json::json!(options));
            let reference: Vec<&str> = body
                .options
                .iter()
                .filter(|o| body.answer.contains(&o.id))
                .map(|o| o.text.as_str())
                .collect();
            state.insert("reference_answers".into(), serde_json::json!(reference));
        }
        QuestionKind::FillBlank(fill) => {
            let reference: Vec<&Vec<String>> = fill.blanks.iter().map(|b| &b.acceptable).collect();
            state.insert("reference_answers".into(), serde_json::json!(reference));
        }
    }
    state.insert(
        "student_answer".into(),
        serde_json::to_value(answer).unwrap_or(serde_json::Value::Null),
    );
    JevState(serde_json::Value::Object(state))
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

/// 学生作答的可读形式（choice → 选项号；fill → 分号串）
fn answer_display(answer: &Answer) -> String {
    match answer {
        Answer::Choice(ids) => ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        Answer::Fill(values) => values.join(" ; "),
    }
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

    #[test]
    fn test_english_and_non_english_thresholds_match_docs() {
        let en = Thresholds::english();
        assert_eq!((en.accept, en.reject), (0.75, 0.35), "docs/05 §3");
        let other = Thresholds::non_english();
        assert_eq!((other.accept, other.reject), (0.85, 0.20), "非英语更保守");
    }

    #[test]
    fn test_for_lang_switches_between_bands() {
        assert_eq!(Thresholds::for_lang("en"), Thresholds::english());
        assert_eq!(Thresholds::for_lang("ja"), Thresholds::non_english());
        assert_eq!(Thresholds::for_lang("zh-Hans"), Thresholds::non_english());
    }

    #[test]
    fn test_non_english_thresholds_use_conservative_band() {
        let other = Thresholds::non_english();
        assert_eq!(
            (other.accept, other.reject),
            (0.85, 0.20),
            "docs/05 §3：其它语种更保守（灰区更宽）"
        );
        assert!(other.accept > Thresholds::english().accept);
        assert!(other.reject < Thresholds::english().reject);
    }

    #[test]
    fn test_new_and_offline_construct_grader() {
        let grader = Grader::new(None, None, Thresholds::english());
        assert!(grader.jev.is_none() && grader.llm.is_none());
        let offline = offline_router();
        assert_eq!(offline.thresholds, Thresholds::english());
        let custom = Grader::new(None, None, Thresholds::non_english());
        assert_eq!(custom.thresholds, Thresholds::non_english());
    }

    #[test]
    fn test_offline_builds_without_ai_clients() {
        let offline = Grader::offline();
        assert!(offline.jev.is_none(), "离线无 Jev 客户端");
        assert!(offline.llm.is_none(), "离线无 LLM 路由");
        assert_eq!(
            offline.thresholds,
            Thresholds::english(),
            "离线默认英语档（grade 时按题目 lang 再切）"
        );
    }

    #[test]
    fn test_effective_thresholds_picks_lang_band_unless_overridden() {
        let grader = Grader::offline();
        let mut ja = choice_question();
        ja.lang = ambition_core::LangTag("ja".into());
        assert_eq!(
            grader.effective_thresholds(&ja),
            Thresholds::non_english(),
            "非英语题切保守档"
        );
        assert_eq!(
            grader.effective_thresholds(&choice_question()),
            Thresholds::english(),
            "英语题默认档"
        );
        let forced = Grader::new(None, None, Thresholds::non_english());
        assert_eq!(
            forced.effective_thresholds(&choice_question()),
            Thresholds::non_english(),
            "显式阈值优先于 lang 推断"
        );
    }

    #[test]
    fn test_downgrade_route_demotes_llm_review_without_llm() {
        let offline = Grader::offline();
        assert_eq!(
            offline.downgrade_route(Route::LlmReview),
            Route::HumanReview,
            "无 LLM → 人工裁决"
        );
        assert_eq!(
            offline.downgrade_route(Route::AutoAccept),
            Route::AutoAccept
        );
        assert_eq!(
            offline.downgrade_route(Route::AutoReject),
            Route::AutoReject
        );
        assert_eq!(
            offline.downgrade_route(Route::HumanReview),
            Route::HumanReview
        );
        let with_llm = Grader::new(None, Some(Arc::new(dummy_router())), Thresholds::english());
        assert_eq!(
            with_llm.downgrade_route(Route::LlmReview),
            Route::LlmReview,
            "有 LLM 保持复核"
        );
    }

    #[test]
    fn test_split_answer_separates_choice_and_fill() {
        let (ids, fills) = split_answer(&Answer::Choice(vec![1, 2]));
        assert_eq!(ids, vec![1, 2]);
        assert!(fills.is_empty());
        let (ids2, fills2) = split_answer(&Answer::Fill(vec!["a".into()]));
        assert!(ids2.is_empty());
        assert_eq!(fills2, vec!["a".to_string()]);
    }

    #[test]
    fn test_pending_grade_marks_low_confidence() {
        let grade = pending_grade(1.0, 2.0, vec![], vec![ErrorTag::Tense]);
        assert_eq!(grade.verdict, Verdict::Pending);
        assert_eq!(grade.confidence, 0.0, "低置信标记");
        assert_eq!(grade.graded_by, GradedBy::Rule);
        assert_eq!((grade.score, grade.max), (1.0, 2.0));
        assert_eq!(grade.error_tags, vec![ErrorTag::Tense]);
    }

    #[test]
    fn test_min_margin_takes_weakest_judgment() {
        assert_eq!(min_margin(&[]), 1.0, "空集恒 1");
        assert_eq!(
            min_margin(&[
                ItemJudge {
                    index: 0,
                    belongs: true,
                    margin: 0.9
                },
                ItemJudge {
                    index: 1,
                    belongs: false,
                    margin: 0.2
                },
            ]),
            0.2
        );
    }

    #[test]
    fn test_answer_state_is_l2_only_and_carries_context() {
        let state = answer_state(&choice_question(), &Answer::Choice(vec![1]));
        let text = state.0.to_string();
        assert!(text.contains("She is quick"), "题干入 state");
        assert!(text.contains("reference_answers"), "参考答案入 state");
        assert!(text.contains("student_answer"), "学生作答入 state");
        assert!(
            !text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "无 CJK（l2-only）: {text}"
        );
        let fill_state = answer_state(&fill_question(), &Answer::Fill(vec!["is".into()]));
        assert!(
            fill_state.0.to_string().contains("\"is\""),
            "填空参考入 state"
        );
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
    fn test_answer_display_renders_choice_ids_and_fill_text() {
        assert_eq!(answer_display(&Answer::Choice(vec![0, 2])), "0,2");
        assert_eq!(
            answer_display(&Answer::Fill(vec!["a".into(), "b".into()])),
            "a ; b"
        );
        assert_eq!(answer_display(&Answer::Choice(vec![])), "", "空选为空串");
        assert_eq!(answer_display(&Answer::Fill(vec![])), "", "空填为空串");
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

    #[test]
    fn test_merge_error_cause_appends_to_questions() {
        let mut questions = JevQuestions::new();
        questions.insert(
            "acceptable".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "fits".into(),
            }),
        );
        let merged = merge_error_cause(questions);
        assert!(merged.contains_key("error_cause"));
        assert!(merged.contains_key("acceptable"));
    }

    /// 无供给方请求的路由器占位（downgrade 判「有 LLM」用；structured 永不被调到）
    fn dummy_router() -> LlmRouter {
        struct NoCall;
        #[async_trait::async_trait]
        impl ambition_llm::LlmProvider for NoCall {
            async fn complete(
                &self,
                req: &ambition_llm::GenRequest,
            ) -> Result<ambition_llm::LlmOut, LlmError> {
                let _ = req.temperature;
                Err(LlmError::Offline)
            }
            fn model(&self) -> &str {
                "no-call"
            }
        }
        LlmRouter::new(
            Box::new(NoCall),
            None,
            Box::new(ambition_llm::MemLedger::new()),
        )
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
    async fn test_grade_fill_jev_flips_variant_to_acceptable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "acceptable": {"type": "noul", "noul": 0.9},
                    "error_cause": {"type": "choice", "choice": "no_error",
                                    "probabilities": {"no_error": 0.9}, "confidence": 0.8}
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
            .grade(&fill_question(), &Answer::Fill(vec!["be".to_string()]))
            .await;
        assert_eq!(grade.verdict, Verdict::Acceptable, "变体经 Jev 翻案");
        assert_eq!((grade.score, grade.max), (1.0, 1.0));
        assert_eq!(grade.graded_by, GradedBy::Jev);
        assert!(grade.error_tags.is_empty(), "翻案成功不带错因");
    }

    #[tokio::test]
    async fn test_grade_fill_jev_confirms_wrong_with_tag_and_partial() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "acceptable": {"type": "noul", "noul": 0.1},
                    "error_cause": {"type": "choice", "choice": "article",
                                    "probabilities": {"article": 0.9}, "confidence": 0.88}
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
            .grade(&fill_question(), &Answer::Fill(vec!["was".to_string()]))
            .await;
        assert_eq!(grade.verdict, Verdict::Wrong, "确认错：维持规则结论");
        assert_eq!(grade.graded_by, GradedBy::Jev);
        assert_eq!(
            grade.error_tags,
            vec![ErrorTag::Article],
            "确认错的空带错因"
        );
        assert!((grade.confidence - 0.1).abs() < 1e-5, "置信 = 最弱 margin");
    }

    #[tokio::test]
    async fn test_grade_fill_jev_gray_band_goes_pending_without_llm() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {"acceptable": {"type": "noul", "noul": 0.5}}
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let grade = grader
            .grade(&fill_question(), &Answer::Fill(vec!["was".to_string()]))
            .await;
        assert_eq!(grade.verdict, Verdict::Pending, "灰区无 LLM → 桶");
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
    async fn test_jev_blank_requires_fill_question_and_range() {
        let grader = offline_router();
        let err = grader
            .jev_blank(&choice_question(), 0, "x")
            .await
            .expect_err("非填空题拒绝");
        assert!(matches!(err, JevError::Invalid(_)), "{err}");
        let err2 = grader
            .jev_blank(&fill_question(), 5, "x")
            .await
            .expect_err("越界拒绝");
        assert!(matches!(err2, JevError::Invalid(_)), "{err2}");
        let err3 = grader
            .jev_blank(&fill_question(), 0, "x")
            .await
            .expect_err("校验通过但离线 → Offline 显式");
        assert!(matches!(err3, JevError::Offline), "{err3}");
    }

    #[tokio::test]
    async fn test_jev_blank_parses_noul_and_tags() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "acceptable": {"type": "noul", "noul": 0.82},
                    "error_cause": {"type": "choice", "choice": "spelling",
                                    "probabilities": {"spelling": 0.8}, "confidence": 0.9}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let judge = grader
            .jev_blank(&fill_question(), 0, "is")
            .await
            .expect("逐空判断");
        assert_eq!(judge.blank_index, 0);
        assert!((judge.yes_prob - 0.82).abs() < 1e-5);
        assert_eq!(judge.error_tags, vec![ErrorTag::Spelling]);
    }

    #[tokio::test]
    async fn test_jev_blank_missing_answer_reports_invalid() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {"unexpected": {"type": "noul", "noul": 0.5}}
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let err = grader
            .jev_blank(&fill_question(), 0, "is")
            .await
            .expect_err("缺 acceptable 显式报错");
        assert!(matches!(err, JevError::Invalid(_)), "{err}");
    }

    #[tokio::test]
    async fn test_jev_multi_items_requires_multi_and_missed_options() {
        let grader = offline_router();
        let err = grader
            .jev_multi_items(&choice_question(), &[0])
            .await
            .expect_err("单选拒绝");
        assert!(matches!(err, JevError::Invalid(_)), "{err}");
        let err2 = grader
            .jev_multi_items(&multi_question(), &[0, 2])
            .await
            .expect_err("无漏选拒绝");
        assert!(matches!(err2, JevError::Invalid(_)), "{err2}");
        let err3 = grader
            .jev_multi_items(&multi_question(), &[0])
            .await
            .expect_err("校验通过但离线 → Offline 显式");
        assert!(matches!(err3, JevError::Offline), "{err3}");
    }

    #[tokio::test]
    async fn test_jev_multi_items_builds_per_missed_judges() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "answers": {
                    "belongs_2": {"type": "noul", "noul": 0.88},
                    "error_cause": {"type": "choice", "choice": "logic",
                                    "probabilities": {"logic": 0.8}, "confidence": 0.8}
                }
            })))
            .mount(&server)
            .await;
        let grader = Grader::new(
            Some(Arc::new(client_with(&server).await)),
            None,
            Thresholds::english(),
        );
        let judge = grader
            .jev_multi_items(&multi_question(), &[0])
            .await
            .expect("逐项判断");
        assert_eq!(judge.items.len(), 1, "只判漏选项");
        assert_eq!(judge.items[0].index, 2);
        assert!(judge.items[0].belongs, "p≥0.5 → 属于正确集");
        assert!((judge.items[0].margin - 0.88).abs() < 1e-5);
        assert_eq!(judge.error_tags, vec![ErrorTag::Logic]);
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
