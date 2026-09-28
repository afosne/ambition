//! Jev 质检回路：QcReport/from_answers/qc_state/qc_questions + 阈值常量（docs/06 §3）

use super::*;

/// 质检 noul 下限（docs/06 §2：noul ≥ 0.7）
pub const QC_NOUL_MIN: f64 = 0.7;

/// 质检 natural 分下限（docs/06 §2：natural ≥ 1.5；score levels 0..2）
pub const QC_NATURAL_MIN: f64 = 1.5;

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

/// 质检 state（只放 l2：题干/选项/答案键；母语/提示永不进 state，docs/05 §3.5）
pub(crate) fn qc_state(
    question: &ambition_core::Question,
    kind: QuestionKindTag,
) -> ambition_jev::JevState {
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
        // M4 题型补全：语篇题带 passage + 逐子题 reference（docs/06 §2 M4 注记）
        (ambition_core::QuestionKind::Cloze(cloze), false) => serde_json::json!({
            "passage": cloze.passage,
            "acceptable_per_blank": cloze
                .blanks
                .iter()
                .map(|b| &b.acceptable)
                .collect::<Vec<_>>(),
            "difficulty": question.difficulty,
        }),
        (ambition_core::QuestionKind::GrammarFill(gf), false) => serde_json::json!({
            "passage": gf.passage,
            "acceptable_per_blank": gf
                .blanks
                .iter()
                .map(|b| &b.acceptable)
                .collect::<Vec<_>>(),
            "difficulty": question.difficulty,
        }),
        (ambition_core::QuestionKind::Reading(reading), false) => serde_json::json!({
            "passage": reading.passage,
            "item_questions": reading.items.iter().map(|i| &i.question).collect::<Vec<_>>(),
            "answer_option_texts_per_item": reading.items.iter().map(|i| {
                i.answer.iter().map(|id| i.options.iter().find(|o| o.id == *id).map(|o| o.text.clone()).unwrap_or_default()).collect::<Vec<_>>()
            }).collect::<Vec<_>>(),
            "difficulty": question.difficulty,
        }),
        (ambition_core::QuestionKind::SevenOfFive(s75), false) => serde_json::json!({
            "passage": s75.passage,
            "options": s75.options,
            "answer_option_texts": s75.answer.iter().map(|id| s75.options.get(*id as usize).cloned().unwrap_or_default()).collect::<Vec<_>>(),
            "difficulty": question.difficulty,
        }),
        // M5-W2：翻译 QC——state 只进 l2（reference 进、源文 l1 永不进，docs/05 §3.5）
        (ambition_core::QuestionKind::Translation(body), false) => serde_json::json!({
            "reference": body.reference,
            "difficulty": question.difficulty,
        }),
        // M5-W3：作文 QC——中文题面不进 state（红线），只看 genre/难度/英文指令
        (ambition_core::QuestionKind::Essay(body), false) => serde_json::json!({
            "stem": question.stem,
            "genre": body.genre,
            "difficulty": question.difficulty,
        }),
        _ => serde_json::json!({"stem": question.stem}),
    };
    ambition_jev::JevState(value)
}

/// 质检问题集（选择类：unique + distractor；全类：key/natural/difficulty_fit）
pub(crate) fn qc_questions(kind: QuestionKindTag) -> JevQuestions {
    let mut qs = JevQuestions::new();
    // 作文无答案键（开放作答）→ key_correct 无意义，跳过（M5-W3）
    if !matches!(kind, QuestionKindTag::Essay) {
        qs.insert(
            "key_correct".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "Is the provided answer key actually correct for this question?"
                    .into(),
            }),
        );
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{BlankItem, FillBody, OptionItem, QuestionKind};

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
    fn test_qc_state_contains_only_l2_fields() {
        let draft = DraftQuestion {
            kind: QuestionKindTag::SingleChoice,
            stem: "Pick one.".into(),
            options: vec!["alpha".into(), "beta".into()],
            answers: vec!["alpha".into()],
            blanks: vec![],
            passage: String::new(),
            items: vec![],
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
}
