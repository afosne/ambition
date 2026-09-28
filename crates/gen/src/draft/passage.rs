//! 语篇题型草稿解析：answer_ids/passage_blanks_body/reading_item_body（M4 题型补全，docs/06 §2）
//! 从 draft.rs 拆出（docs/01 §组织：文件 ≤500 行）

use super::*;

/// 答案文本 → 选项下标（选项文本匹配；未找到/重复 → Err）
pub(crate) fn answer_ids(answers: &[String], options: &[String]) -> Result<Vec<u32>, GenError> {
    if answers.is_empty() {
        return Err(GenError::InvalidDraft("no answers".into()));
    }
    let mut ids = Vec::with_capacity(answers.len());
    for ans in answers {
        let id = options
            .iter()
            .position(|o| o.trim() == ans.trim())
            .ok_or_else(|| GenError::InvalidDraft(format!("answer not in options: {ans}")))?
            as u32;
        if ids.contains(&id) {
            return Err(GenError::InvalidDraft(format!("duplicate answer: {ans}")));
        }
        ids.push(id);
    }
    Ok(ids)
}

/// 语篇 + 逐空（完形/语法填空共用；空位 = `___` 标记按序；M4 题型补全）
pub(crate) fn passage_blanks_body(
    draft: &DraftQuestion,
    kind: &str,
) -> Result<ambition_core::ClozeBody, GenError> {
    if draft.passage.trim().is_empty() {
        return Err(GenError::InvalidDraft(format!("{kind} needs passage")));
    }
    if draft.blanks.is_empty() {
        return Err(GenError::InvalidDraft(format!("{kind} needs blanks")));
    }
    let markers = draft.passage.matches("___").count();
    if markers != draft.blanks.len() {
        return Err(GenError::InvalidDraft(format!(
            "{kind} passage has {markers} blanks but {} answer sets",
            draft.blanks.len()
        )));
    }
    if draft.blanks.iter().any(|set| set.is_empty()) {
        return Err(GenError::InvalidDraft("blank without acceptable".into()));
    }
    Ok(ambition_core::ClozeBody {
        passage: draft.passage.clone(),
        blanks: draft
            .blanks
            .iter()
            .map(|set| ambition_core::BlankItem {
                acceptable: set.clone(),
            })
            .collect(),
    })
}

/// 阅读子题 → ChoiceBody（选项 + 答案下标；M4 题型补全）
pub(crate) fn reading_item_body(
    item: &DraftReadingItem,
) -> Result<ambition_core::ChoiceBody, GenError> {
    if item.question.trim().is_empty() {
        return Err(GenError::InvalidDraft("reading item empty question".into()));
    }
    if item.options.len() < 2 {
        return Err(GenError::InvalidDraft(
            "reading item needs >=2 options".into(),
        ));
    }
    let answer = answer_ids(&item.answers, &item.options)?;
    if answer.len() != 1 {
        return Err(GenError::InvalidDraft(format!(
            "reading item needs exactly 1 answer, got {}",
            answer.len()
        )));
    }
    Ok(ambition_core::ChoiceBody {
        options: item
            .options
            .iter()
            .enumerate()
            .map(|(i, text)| ambition_core::OptionItem {
                id: i as u32,
                text: text.clone(),
            })
            .collect(),
        answer,
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_answer_ids_maps_text_and_rejects_unknown_or_duplicate() {
        let opts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let ids = answer_ids(&["c".to_string(), "a".to_string()], &opts).expect("映射");
        assert_eq!(ids, vec![2, 0]);
        assert!(
            answer_ids(&["z".to_string()], &opts).is_err(),
            "未知选项 → Err"
        );
        assert!(
            answer_ids(&["a".to_string(), "a".to_string()], &opts).is_err(),
            "重复 → Err"
        );
        assert!(answer_ids(&[], &opts).is_err(), "空答案 → Err");
    }

    #[test]
    fn test_passage_blanks_body_counts_markers_and_rejects_mismatch() {
        let draft: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "cloze", "stem": "s", "passage": "I ___ a. She ___ .",
            "blanks": [["am"], ["runs"]], "difficulty": 2, "tags": []
        }))
        .expect("草稿");
        let body = passage_blanks_body(&draft, "cloze").expect("解析");
        assert_eq!(body.blanks.len(), 2);
        let bad: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "cloze", "stem": "s", "passage": "I ___ a.",
            "blanks": [["am"], ["runs"]], "difficulty": 2, "tags": []
        }))
        .expect("草稿");
        assert!(
            passage_blanks_body(&bad, "cloze").is_err(),
            "空位数不符 → Err"
        );
        let no_passage: DraftQuestion = serde_json::from_value(serde_json::json!({
            "kind": "cloze", "stem": "s", "passage": " ",
            "blanks": [["am"]], "difficulty": 2, "tags": []
        }))
        .expect("草稿");
        assert!(
            passage_blanks_body(&no_passage, "cloze").is_err(),
            "空 passage → Err"
        );
    }

    #[test]
    fn test_reading_item_body_requires_two_options_one_answer() {
        let item: DraftReadingItem = serde_json::from_value(serde_json::json!({
            "question": "What?", "options": ["a", "b"], "answers": ["b"]
        }))
        .expect("子题");
        let body = reading_item_body(&item).expect("解析");
        assert_eq!(body.answer, vec![1]);
        assert_eq!(body.options.len(), 2);
        let too_few: DraftReadingItem = serde_json::from_value(serde_json::json!({
            "question": "What?", "options": ["only"], "answers": ["only"]
        }))
        .expect("子题");
        assert!(reading_item_body(&too_few).is_err(), "<2 选项 → Err");
        let two_answers: DraftReadingItem = serde_json::from_value(serde_json::json!({
            "question": "What?", "options": ["a", "b"], "answers": ["a", "b"]
        }))
        .expect("子题");
        assert!(reading_item_body(&two_answers).is_err(), "多答案 → Err");
    }
}
