//! wire 编解码：encode_question/encode_request/decode_answer/decode_response（状态保 CJK-free）

use super::*;

/// 序列化问题 → wire 形态（{type, instructions, criteria}）
pub(crate) fn encode_question(q: &JevQuestion) -> serde_json::Value {
    match q {
        JevQuestion::Noul(NoulQ { instructions }) => {
            serde_json::json!({"type": "noul", "instructions": instructions})
        }
        JevQuestion::Choice(ChoiceQ {
            instructions,
            criteria,
        }) => {
            serde_json::json!({"type": "choice", "instructions": instructions, "criteria": criteria})
        }
        JevQuestion::Score(ScoreQ {
            instructions,
            levels,
        }) => {
            serde_json::json!({"type": "score", "instructions": instructions, "criteria": levels})
        }
    }
}

/// 组请求体（state 只含 l2；model 默认 jev-latest）
pub(crate) fn encode_request(
    state: &JevState,
    model: &str,
    questions: &JevQuestions,
) -> serde_json::Value {
    let encoded: serde_json::Map<String, serde_json::Value> = questions
        .iter()
        .map(|(id, q)| (id.clone(), encode_question(q)))
        .collect();
    serde_json::json!({"state": state.0, "model": model, "questions": encoded})
}

/// 解析回答（type 字段分派；缺 type = 响应不符契约）
pub(crate) fn decode_answer(value: &serde_json::Value) -> Option<JevAnswer> {
    match value.get("type")?.as_str()? {
        "noul" => Some(JevAnswer::Noul {
            yes_prob: value.get("noul")?.as_f64()?,
        }),
        "choice" => {
            let choice = value.get("choice")?.as_str()?.to_string();
            let probabilities = value
                .get("probabilities")?
                .as_object()?
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                .collect();
            Some(JevAnswer::Choice {
                choice,
                probabilities,
                confidence: value.get("confidence")?.as_f64()?,
            })
        }
        "score" => Some(JevAnswer::Score {
            score: value.get("score")?.as_f64()?,
            confidence: value.get("confidence")?.as_f64()?,
        }),
        _ => None,
    }
}

/// 解析响应体（answers 缺失/单条损坏 → Err）
pub(crate) fn decode_response(body: &serde_json::Value) -> Result<JevAnswers, JevError> {
    let answers = body
        .get("answers")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| JevError::Invalid("missing answers".into()))?;
    let mut out = JevAnswers::new();
    for (id, value) in answers {
        let answer =
            decode_answer(value).ok_or_else(|| JevError::Invalid(format!("bad answer: {id}")))?;
        out.insert(id.clone(), answer);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_questions() -> JevQuestions {
        let mut qs = JevQuestions::new();
        qs.insert(
            "unique_answer".into(),
            JevQuestion::Noul(NoulQ {
                instructions: "Is exactly one option correct?".into(),
            }),
        );
        qs.insert(
            "natural".into(),
            JevQuestion::Score(ScoreQ {
                instructions: "How natural is the sentence?".into(),
                levels: vec!["unnatural".into(), "ok".into(), "natural".into()],
            }),
        );
        qs.insert(
            "error_cause".into(),
            JevQuestion::Choice(ChoiceQ {
                instructions: "Classify the mistake.".into(),
                criteria: [("word_sense".to_string(), "wrong sense".to_string())]
                    .into_iter()
                    .collect(),
            }),
        );
        qs
    }

    fn ok_body() -> serde_json::Value {
        serde_json::json!({
            "model": "jev-1.13.0",
            "answers": {
                "unique_answer": {"type": "noul", "noul": 0.95},
                "natural": {"type": "score", "score": 1.9, "confidence": 0.9,
                            "legend": {"0":"unnatural","1":"ok","2":"natural"},
                            "probabilities": {"0":0.0,"1":0.05,"2":0.95}},
                "error_cause": {"type": "choice", "choice": "word_sense",
                                "probabilities": {"word_sense": 0.9}, "confidence": 0.88}
            },
            "usage": {"input_tokens": 300, "output_tokens": 25}
        })
    }

    #[test]
    fn test_encode_question_covers_three_primitives() {
        let qs = sample_questions();
        let body = encode_request(&JevState(serde_json::json!("stem")), "jev-latest", &qs);
        assert_eq!(body["model"], "jev-latest");
        assert_eq!(body["questions"]["unique_answer"]["type"], "noul");
        assert_eq!(body["questions"]["natural"]["type"], "score");
        assert_eq!(
            body["questions"]["natural"]["criteria"][2], "natural",
            "score criteria = 有序 levels 数组"
        );
        assert_eq!(body["questions"]["error_cause"]["type"], "choice");
        assert_eq!(
            body["questions"]["error_cause"]["criteria"]["word_sense"],
            "wrong sense"
        );
        assert_eq!(body["state"], "stem", "state 原样透传");
    }

    #[test]
    fn test_encode_request_keeps_state_cjk_free() {
        // docs/05 §3.5：state 只放 l2 —— 此断言保证调用方拼的 state 未混入母语提示
        let state = JevState(serde_json::json!({
            "sentence": "She is ___ student.",
            "options": ["a", "an", "the"],
            "answer": "an"
        }));
        let body = encode_request(&state, "jev-latest", &sample_questions());
        let text = body.to_string();
        assert!(
            !text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "state 无 CJK 母语内容: {text}"
        );
    }

    #[test]
    fn test_decode_response_parses_all_primitives_and_rejects_garbage() {
        let answers = decode_response(&ok_body()).expect("解析");
        assert_eq!(
            answers.get("unique_answer").and_then(JevAnswer::noul_prob),
            Some(0.95),
            "noul → yes_prob"
        );
        assert_eq!(
            answers.get("natural").and_then(JevAnswer::score_value),
            Some(1.9)
        );
        match answers.get("error_cause") {
            Some(JevAnswer::Choice {
                choice, confidence, ..
            }) => {
                assert_eq!(choice, "word_sense");
                assert!((*confidence - 0.88).abs() < 1e-9);
            }
            other => panic!("choice 解析错误: {other:?}"),
        }

        let missing = serde_json::json!({"model": "x"});
        assert!(decode_response(&missing).is_err(), "缺 answers 报错");
        let broken = serde_json::json!({"answers": {"a": {"type": "mystery"}}});
        assert!(decode_response(&broken).is_err(), "未知 type 报错");
    }

    #[test]
    fn test_decode_answer_dispatches_by_type_field() {
        let noul = serde_json::json!({"type": "noul", "noul": 0.4});
        assert_eq!(decode_answer(&noul).and_then(|a| a.noul_prob()), Some(0.4));
        assert!(
            decode_answer(&serde_json::json!({"type": "unknown"})).is_none(),
            "未知 type → None"
        );
        assert!(
            decode_answer(&serde_json::json!({"type": "noul"})).is_none(),
            "缺字段 → None"
        );
    }
}
