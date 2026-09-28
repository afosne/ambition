//! LLM 草稿解析与落题：DraftQuestion/to_question/choice_body/parse_draft_batch（docs/06 §2）
//! 本域含语篇解析子组件（`draft::passage`，docs/01 §1 细分功能目录）。

mod passage;

use self::passage::{answer_ids, passage_blanks_body, reading_item_body};
use crate::*;

/// 阅读子题草稿（M4 题型补全，docs/06 §1）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DraftReadingItem {
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub answers: Vec<String>,
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
    /// 语篇（完形/阅读/七选五/语法填空；M4 题型补全）
    #[serde(default)]
    pub passage: String,
    /// 阅读子题（M4 题型补全）
    #[serde(default)]
    pub items: Vec<DraftReadingItem>,
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
            // M4 题型补全：完形/语法填空（语篇 + 逐空；空位 = `___` 标记按序）
            QuestionKindTag::Cloze => {
                let body = passage_blanks_body(self, "cloze")?;
                ambition_core::QuestionKind::Cloze(body)
            }
            QuestionKindTag::GrammarFill => {
                let cloze = passage_blanks_body(self, "grammar_fill")?;
                ambition_core::QuestionKind::GrammarFill(ambition_core::GrammarFillBody {
                    passage: cloze.passage,
                    blanks: cloze.blanks,
                })
            }
            // 阅读：语篇 + 子题（每子题单选）
            QuestionKindTag::Reading => {
                if self.passage.trim().is_empty() {
                    return Err(GenError::InvalidDraft("reading needs passage".into()));
                }
                if self.items.is_empty() {
                    return Err(GenError::InvalidDraft("reading needs items".into()));
                }
                let mut items = Vec::with_capacity(self.items.len());
                for item in &self.items {
                    let body = reading_item_body(item)?;
                    items.push(ambition_core::ReadingItem {
                        question: item.question.clone(),
                        options: body.options,
                        answer: body.answer,
                    });
                }
                ambition_core::QuestionKind::Reading(ambition_core::ReadingBody {
                    passage: self.passage.clone(),
                    items,
                })
            }
            // 七选五：语篇（5 缺口）+ 7 候选句 + answer（按缺口顺序的选中句，长度 5）
            QuestionKindTag::SevenOfFive => {
                if self.passage.trim().is_empty() {
                    return Err(GenError::InvalidDraft("seven_of_five needs passage".into()));
                }
                if self.options.len() < 7 {
                    return Err(GenError::InvalidDraft(
                        "seven_of_five needs >=7 options".into(),
                    ));
                }
                let markers = self.passage.matches("___").count();
                if markers != 5 {
                    return Err(GenError::InvalidDraft(format!(
                        "seven_of_five passage needs 5 blanks, got {markers}"
                    )));
                }
                if self.answers.len() != 5 {
                    return Err(GenError::InvalidDraft(format!(
                        "seven_of_five needs 5 answers, got {}",
                        self.answers.len()
                    )));
                }
                let answer = answer_ids(&self.answers, &self.options)?;
                ambition_core::QuestionKind::SevenOfFive(ambition_core::SevenOfFiveBody {
                    passage: self.passage.clone(),
                    options: self.options.clone(),
                    answer,
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
    #[error("chinglish corpus: {0}")]
    Chinglish(String),
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
    use ambition_core::{Provenance, QuestionKind};

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
    fn test_to_question_maps_choice_answers_by_text() {
        let draft = DraftQuestion {
            kind: QuestionKindTag::MultiChoice,
            stem: "Pick two.".into(),
            options: vec!["x".into(), "y".into(), "z".into()],
            answers: vec!["z".into(), "x".into()],
            blanks: vec![],
            passage: String::new(),
            items: vec![],
            difficulty: 2,
            tags: vec![],
        };
        let body = choice_body(&draft).expect("映射");
        assert_eq!(body.answer, vec![2, 0], "按文本映射回 id");
        assert_eq!(body.options[0].id, 0);
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

    #[test]
    fn test_choice_body_maps_answers_by_text() {
        let draft = DraftQuestion {
            kind: QuestionKindTag::MultiChoice,
            stem: "Pick two.".into(),
            options: vec!["x".into(), "y".into(), "z".into()],
            answers: vec!["z".into(), "x".into()],
            blanks: vec![],
            passage: String::new(),
            items: vec![],
            difficulty: 2,
            tags: vec![],
        };
        let body = choice_body(&draft).expect("映射");
        assert_eq!(body.answer, vec![2, 0], "按文本映射回 id");
        assert_eq!(body.options[0].id, 0);
    }
}
